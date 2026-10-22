#!/usr/bin/env python3
"""Real MCP stdio and RTK HTTP processes with frozen official page content.
Requests terminate in a local fake provider; no model, credential or external HTTP call.
"""
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
import json,signal,subprocess,sys,tempfile,threading,urllib.request
from verify import HERE,REPO,OLD,NEW,SELECTOR,load,save,invoke,sha

def compact(v):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
def cases():
    result=[]
    for base,keys,english_file,budget in [(OLD,['redis','s3'],'generated_english.json',1024),(NEW,['queue'],'improved_english.json',2048)]:
        qs={c['id']:c for c in load(base/'questions.json')['questions']}
        en={c['id']:c['query_en'] for c in load(base/english_file)}
        for key in keys:
            result.append({'id':key,'source':load(base/f'{key}-page.json'),'query':qs[key]['question_ja'],
                'english':en[key],'budget':budget,'tool':'fetch','adapter':'markdown-page','approved':base==OLD})
    source={'results':[{'id':'n','url':'https://example.test/n','title':'Limits','text':'Non-positive durations delete immediately.'}]}
    for key,english in [('search-unapproved','zero or negative'),('search-improved','zero or negative (non-positive)')]:
        result.append({'id':key,'source':source,'query':'ゼロ負数時の扱い','english':english,'budget':1024,'tool':'search',
            'adapter':'search-results','approved':False})
    return result

def expected(case):
    req={'query':case['query'],'query_expansions':[case['english']],'max_tokens':case['budget'],'input':case['source']}
    value=invoke(req,adapter=case['adapter'])
    off=invoke(req,'--query-rules','off',adapter=case['adapter'])
    assert (value!=off)==case['approved'],case['id']
    if case['id']=='queue':
        assert {b['id'] for d in value['documents'] for b in d['blocks']} >= {'line-45','line-47','line-110','line-112'}
    if case['id']=='search-unapproved': assert value['status']=='no_lexical_overlap'
    if case['id']=='search-improved': assert value['documents'][0]['id']=='n'
    return value

def main():
    all_cases=cases();fixtures={c['source']['url'] if c['tool']=='fetch' else c['id']:c['source'] for c in all_cases}
    rows=[];forwarded=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def do_POST(self):
            forwarded.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            body=compact({'id':'local','type':'message','role':'assistant','model':'fixture','content':[{'type':'text','text':'ok'}],
                'stop_reason':'end_turn','usage':{'input_tokens':0,'output_tokens':0}}).encode()
            self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)))
            self.end_headers();self.wfile.write(body)
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    descriptions={}
    try:
        with tempfile.TemporaryDirectory() as directory:
            directory=Path(directory);save(directory/'fixtures.json',fixtures)
            for budget in [1024,2048]:
                proxy=subprocess.Popen([sys.executable,str(REPO/'integrations/mcp/jrg_select_proxy.py'),'--selector',str(SELECTOR),
                    '--max-tokens',str(budget),'--upstream',sys.executable,str(HERE/'replay_mcp.py'),str(directory/'fixtures.json'),str(directory/'upstream.jsonl')],
                    stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
                def rpc(method,params):
                    proxy.stdin.write(compact({'jsonrpc':'2.0','id':method,'method':method,'params':params})+'\n');proxy.stdin.flush()
                    response=json.loads(proxy.stdout.readline());assert 'error' not in response,response
                    return response['result']
                try:
                    rpc('initialize',{'protocolVersion':'2025-03-26'})
                    tools=rpc('tools/list',{})['tools']
                    for tool in tools:
                        description=tool['inputSchema']['properties']['jrg_focus_en']['description']
                        assert 'before reading tool output' in description and 'numerical bounds' in description
                        descriptions[tool['name']]=description
                    for case in [c for c in all_cases if c['budget']==budget]:
                        argkey='url' if case['tool']=='fetch' else 'query';target=case['source']['url'] if argkey=='url' else case['id']
                        result=rpc('tools/call',{'name':case['tool'],'arguments':{argkey:target,'jrg_focus':case['query'],'jrg_focus_en':case['english']}})
                        assert set(result)=={'content'} and len(result['content'])==1,result
                        selected=json.loads(result['content'][0]['text']);assert selected==expected(case),case['id']
                        rows.append({'transport':'MCP','case':case['id'],'budget':budget,'matches_standard_cli':True,'approved_rule_applies':case['approved']})
                finally:
                    proxy.stdin.close();proxy.stdout.close();assert proxy.wait(timeout=15)==0
                binary=REPO/'integrations/rtk-proxy/target/debug/jrg-rtk-proxy'
                rtk=subprocess.Popen([str(binary),'--selector',str(SELECTOR),'--bind','127.0.0.1:0','--max-tokens',str(budget),
                    '--anthropic-upstream',f'http://127.0.0.1:{server.server_port}'],stderr=subprocess.PIPE,text=True)
                try:
                    line=rtk.stderr.readline();assert 'listening on ' in line,line
                    address=line.split('listening on ',1)[1].split(';',1)[0]
                    for case in [c for c in all_cases if c['budget']==budget]:
                        metadata={'jrg_focus':case['query'],'jrg_focus_en':case['english']}
                        tool={'name':case['tool'],'description':'fixture tool','input_schema':{'type':'object','properties':{'jrg_focus_en':{'type':'string'}}}}
                        request={'model':'fixture-only','max_tokens':10,'tools':[tool],'messages':[
                            {'role':'user','content':case['query']},
                            {'role':'assistant','content':[{'type':'tool_use','id':case['id'],'name':case['tool'],'input':metadata}]},
                            {'role':'user','content':[{'type':'tool_result','tool_use_id':case['id'],'content':compact(case['source'])}]}]}
                        http=urllib.request.Request(f'http://{address}/v1/messages',data=compact(request).encode(),
                            headers={'Content-Type':'application/json','x-api-key':'local-fixture','anthropic-version':'2023-06-01'})
                        with urllib.request.urlopen(http,timeout=20) as response: assert response.status==200
                        sent=forwarded[-1];block=sent['messages'][-1]['content'][0]
                        assert not block['is_error'],block
                        selected=json.loads(block['content'])['selection'];assert selected==expected(case),case['id']
                        assert sent['messages'][1]['content'][0]['input']==metadata
                        assert sent['tools'][0]['input_schema']['properties']['jrg_focus_en']['description']==descriptions[case['tool']]
                        # Replay history must reuse the identical selected content.
                        with urllib.request.urlopen(http,timeout=20) as response: assert response.status==200
                        assert forwarded[-1]['messages'][-1]['content'][0]==block
                        rows.append({'transport':'RTK','case':case['id'],'budget':budget,'matches_standard_cli':True,
                            'approved_rule_applies':case['approved'],'replayed_identically':True,'english_instruction_matches_mcp':True})
                finally:
                    rtk.send_signal(signal.SIGINT);assert rtk.wait(timeout=15)==0;rtk.stderr.close()
            assert len(load_lines(directory/'upstream.jsonl'))==5
    finally:server.shutdown();server.server_close()
    save(HERE/'transport_results.json',{'rows':rows,'selector_sha256':sha(SELECTOR),
        'rtk_sha256':sha(REPO/'integrations/rtk-proxy/target/debug/jrg-rtk-proxy'),
        'mcp_sha256':sha(REPO/'integrations/mcp/jrg_select_proxy.py'),'external_model_calls':0})
    print(compact(rows))

def load_lines(path):return [json.loads(line) for line in path.read_text().splitlines()]
if __name__=='__main__':main()
