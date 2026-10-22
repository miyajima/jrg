#!/usr/bin/env python3
"""Local stdio upstream for transport verification; never fetches external sources."""
import json, sys
from pathlib import Path
fixtures=json.loads(Path(sys.argv[1]).read_text())
for line in sys.stdin:
    message=json.loads(line);method=message.get('method')
    if 'id' not in message: continue
    if method=='initialize':
        result={'protocolVersion':'2025-03-26','capabilities':{'tools':{}},'serverInfo':{'name':'captured-evidence','version':'1'}}
    elif method=='tools/list':
        result={'tools':[{'name':name,'inputSchema':{'type':'object','properties':{'query' if name=='search' else 'url':{'type':'string'}}}} for name in ['search','fetch']]}
    elif method=='tools/call':
        arguments=message['params']['arguments']
        assert 'jrg_focus' not in arguments and 'jrg_focus_en' not in arguments
        with Path(sys.argv[2]).open('a') as log: log.write(json.dumps(arguments)+'\n')
        source=fixtures[arguments.get('url',arguments.get('query'))]
        result={'content':[{'type':'text','text':json.dumps(source)}],'structuredContent':source}
    else: raise RuntimeError(method)
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}),flush=True)
