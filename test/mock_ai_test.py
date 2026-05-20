#!/usr/bin/env python3
"""AI 代理端到端测试：启动 mock 上游 → 创建渠道 → 生成 token → 充值 → 调用 AI → 验证扣费"""
import asyncio, aiohttp, json, sys
from aiohttp import web

API = 'http://127.0.0.1:7001/yy_easy_aiapi'
API_TOKEN = 'token001'
MOCK_PORT = 9000

async def mock_handler(request):
    body = await request.read()
    req = json.loads(body) if body else {}
    print(f'  [MOCK] 收到: model={req.get("model","?")}, messages={len(req.get("messages",[]))}条')
    resp = {
        'id': 'chatcmpl-mock-001',
        'object': 'chat.completion',
        'created': 1716220800,
        'model': req.get('model', 'mock-model'),
        'choices': [{
            'index': 0,
            'message': {'role': 'assistant', 'content': 'Hello from mock!'},
            'finish_reason': 'stop'
        }],
        'usage': {
            'prompt_tokens': 10,
            'completion_tokens': 10,
            'total_tokens': 20,
            'prompt_tokens_details': {'cached_tokens': 10}
        }
    }
    print(f'  [MOCK] 返回: prompt=10 cached=10 output=10')
    return web.json_response(resp)

async def api(method, path, data=None, token=None):
    h = {'Authorization': f'Bearer {token or API_TOKEN}'}
    if data: h['Content-Type'] = 'application/json'
    async with aiohttp.ClientSession() as s:
        async with s.request(method, f'{API}{path}', json=data, headers=h) as r:
            body = await r.text()
            try: j = json.loads(body)
            except: j = {'raw': body}
            return r.status, j

async def main():
    app_mock = web.Application()
    app_mock.router.add_post('/v1/chat/completions', mock_handler)
    runner = web.AppRunner(app_mock)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', MOCK_PORT)
    await site.start()
    print(f'[MOCK] http://127.0.0.1:{MOCK_PORT}/v1/chat/completions')

    try:
        print()
        status, j = await api('POST', '/api/channels', {
            'name': 'mock-channel',
            'api_standard': 'openai',
            'price_input': '0.01',
            'price_output': '0.03',
            'upstream_url': f'http://127.0.0.1:{MOCK_PORT}',
            'api_key': 'sk-mock'
        })
        print(f'[1] 创建渠道: {status} {j.get("data",{}).get("name","?")}')

        status, j = await api('POST', '/api/token/generate', {'label': 'ai-test'})
        token_str = j['data']['token']
        print(f'[2] 生成 Token: {token_str}')

        status, j = await api('POST', '/api/token/recharge', {'token': token_str, 'amount': '100'})
        print(f'[3] 充值: {status} {j.get("data",{}).get("message","?")}')

        status, j = await api('GET', f'/api/token/balance?token={token_str}')
        bal_before = j['data'][0]['balance']
        print(f'[4] 余额(前): {bal_before}')

        print()
        print('[5] 调用 AI 代理...')
        status, j = await api('POST', '/v1/chat/completions',
            {'model': 'mock-channel', 'messages': [{'role': 'user', 'content': 'hi'}]},
            token=token_str)
        print(f'  HTTP {status}')
        if status == 200:
            c = j.get('choices', [{}])[0]
            print(f'  content: {c.get("message",{}).get("content","?")}')
            print(f'  usage: {j.get("usage", {})}')
        else:
            print(f'  错误: {j}')

        status, j = await api('GET', f'/api/token/balance?token={token_str}')
        bal_after = j['data'][0]['balance']
        print(f'[6] 余额(后): {bal_after}  (差额={float(bal_before)-float(bal_after):.6f})')

        status, j = await api('GET', f'/api/token/transactions?token={token_str}')
        items = j['data']['items']
        print(f'[7] 流水 {len(items)} 条:')
        for t in items:
            print(f'    {t["change_type"]:8s}  amount={t["amount"]:>10s}  balance_after={t["balance_after"]:>10s}  channel={t["channel_name"]}')

    finally:
        await runner.cleanup()
        print()
        print('[MOCK] 关闭')

if __name__ == '__main__':
    if sys.platform == 'win32':
        asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())
    asyncio.run(main())
