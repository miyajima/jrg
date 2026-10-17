"""Offline EmbeddingGemma 2 candidate index. Explicit local Unix socket, no HTTP/API.

Install sentence-transformers>=6.1 and transformers>=5.18 in an isolated venv.
Download the pinned official snapshot separately; serving uses local_files_only.
Run --help for configuration. Cache contains private source vectors; keep it local.
"""
import argparse,hashlib,json,math,os,pathlib,re,signal,socketserver,sqlite3,struct,time
MODEL='google/embeddinggemma-2'
REVISION='914f7f89142e33e77833254d9c9b90c3cef7303b'
MAX_SOURCE=4*1024*1024
MAX_TOTAL=64*1024*1024
DIMENSION=768
class Index:
 def __init__(self,cache,encoder,revision=REVISION):
  self.encoder=encoder;self.revision=revision
  self.db=sqlite3.connect(cache)
  self.db.execute('CREATE TABLE IF NOT EXISTS vectors(key TEXT PRIMARY KEY, vector BLOB)')
  self.last_stats={}
 def search(self,files,query):
  started=time.monotonic();records=[];missing={};total=0;skipped=0
  for name in sorted(set(files)):
   p=pathlib.Path(name)
   if p.is_symlink()or not p.is_file()or p.stat().st_size>MAX_SOURCE:skipped+=1;continue
   raw=p.read_bytes();total+=len(raw)
   if total>MAX_TOTAL:raise ValueError('index input exceeds 64 MiB')
   if b'\0'in raw:skipped+=1;continue
   try:source=raw.decode('utf-8-sig')
   except UnicodeDecodeError:skipped+=1;continue
   lines=source.splitlines();start=0
   while start<len(lines):
    end=start;size=0
    while end<len(lines)and end-start<80 and size+len(lines[end])<=4000:size+=len(lines[end])+1;end+=1
    if end==start: end=start+1
    content='\n'.join(lines[start:end])
    # Document uses the official title:none prompt; path is plain document content.
    text=str(p.name)+'\n'+content
    key=hashlib.sha256((self.revision+'|Document|windows80-4000-overlap20|'+text).encode()).hexdigest()
    row=self.db.execute('SELECT vector FROM vectors WHERE key=?',(key,)).fetchone()
    vector=list(struct.unpack('<'+'f'*(len(row[0])//4),row[0]))if row else None
    if vector is None:missing[key]=text
    records.append({'path':str(p),'start':start,'end':end,'lines':lines[start:end],'key':key,'vector':vector})
    if end==len(lines):break
    start=max(start+1,end-20)
  encoded={};items=list(missing.items())
  for offset in range(0,len(items),16):
   batch=items[offset:offset+16];vectors=self.encoder.encode([text for _,text in batch],prompt_name='Document')
   for (key,_),v in zip(batch,vectors):
    v=[float(x)for x in v];encoded[key]=v
    self.db.execute('INSERT OR REPLACE INTO vectors VALUES(?,?)',(key,struct.pack('<'+'f'*len(v),*v)))
   self.db.commit()
  index_seconds=time.monotonic()-started
  q=[float(x)for x in self.encoder.encode([query],prompt_name='CodeRetrieval')[0]]
  qnorm=math.sqrt(sum(x*x for x in q))or 1.
  for r in records:
   v=r['vector']if r['vector']is not None else encoded[r['key']]
   if len(v)!=len(q)or not all(math.isfinite(x)for x in v+q):raise ValueError('invalid embedding dimensions/values')
   norm=math.sqrt(sum(x*x for x in v))or 1.
   r['score']=sum(x*y for x,y in zip(v,q))/(norm*qnorm)
  records.sort(key=lambda r:(-r['score'],r['path'],r['start']))
  hits=[];counts={};words=set(re.findall(r'[\w]+',query.lower()))
  for r in records:
   if r['score']<=0 or counts.get(r['path'],0)>=2:continue
   counts[r['path']]=counts.get(r['path'],0)+1
   lines=r['lines'];best=max(range(len(lines)),key=lambda i:(len(words&set(re.findall(r'[\w]+',lines[i].lower()))),bool(re.search(r'\b(fn|def|function)\s+\w+|\b(const|let)\s+\w+.*=>',lines[i])),-i))
   hits.append({'path':r['path'],'line':r['start']+best+1,'text':lines[best],'score':r['score']})
   if len(hits)>=24:break
  self.last_stats={'files':len(set(files)),'chunks':len(records),'encoded_documents':len(missing),'skipped':skipped,'index_seconds':index_seconds,'seconds':time.monotonic()-started}
  return hits
class Encoder:
 def __init__(self,model_path,device):
  import torch
  from sentence_transformers import SentenceTransformer
  self.model=SentenceTransformer(model_path,local_files_only=True,device=device,config_kwargs={'vision_config':None,'audio_config':None},model_kwargs={'dtype':torch.float32,'attn_implementation':'eager'})
  if self.model.get_embedding_dimension()!=DIMENSION:raise ValueError('Expected EmbeddingGemma 2 768 dimensions')
  self.model.max_seq_length=2048
 def encode(self,texts,prompt_name):
  return self.model.encode(texts,prompt_name=prompt_name,batch_size=4,normalize_embeddings=True,show_progress_bar=False)
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--model',required=True);ap.add_argument('--cache',required=True);ap.add_argument('--socket',required=True);ap.add_argument('--log',required=True);ap.add_argument('--device',choices=['cpu','mps'],default='mps');a=ap.parse_args()
 model=pathlib.Path(a.model).resolve();config=json.loads((model/'config.json').read_text());prompts=json.loads((model/'config_sentence_transformers.json').read_text())['prompts']
 if model.name!=REVISION or config.get('model_type')!='embedding_gemma2'or prompts.get('CodeRetrieval')!='task: code retrieval | query: 'or prompts.get('Document')!='title: none | text: ':raise ValueError('Official pinned EmbeddingGemma 2 snapshot required')
 os.umask(0o077)
 start=time.monotonic();idx=Index(a.cache,Encoder(str(model),a.device));load_seconds=time.monotonic()-start
 def log(data):
  with open(a.log,'a')as f:f.write(json.dumps(data)+'\n')
 log({'event':'loaded','model':MODEL,'revision':REVISION,'dimension':DIMENSION,'device':a.device,'dtype':'float32','load_seconds':load_seconds,'max_seq_length':2048})
 class Handler(socketserver.StreamRequestHandler):
  def handle(self):
   start=time.monotonic()
   try:
    raw=self.rfile.readline(2*1024*1024+1)
    if len(raw)>2*1024*1024:raise ValueError('request exceeds 2 MiB')
    req=json.loads(raw)
    if req.get('version')!=1 or not isinstance(req.get('files'),list)or len(req['files'])>8192 or not isinstance(req.get('query'),str)or len(req['query'])>4096:raise ValueError('invalid request')
    hits=idx.search(req['files'],req['query']);result={'version':1,'model':MODEL,'revision':REVISION,'hits':hits,'stats':idx.last_stats};log({'event':'search',**idx.last_stats})
   except Exception as e:result={'error':str(e)};log({'event':'error','error':str(e),'seconds':time.monotonic()-start})
   self.wfile.write(json.dumps(result).encode()+b'\n')
 def stop(signum, frame): raise SystemExit(0)
 signal.signal(signal.SIGTERM,stop)
 with socketserver.UnixStreamServer(a.socket,Handler)as server:
  print(json.dumps({'ready':True,'socket':a.socket,'load_seconds':load_seconds}),flush=True)
  try:server.serve_forever()
  finally:pathlib.Path(a.socket).unlink(missing_ok=True)
if __name__=='__main__':main()
