import importlib.util,json,pathlib,tempfile,unittest
spec=importlib.util.spec_from_file_location('embedding',pathlib.Path(__file__).with_name('embedding.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Opener:
 def __init__(self,callback):self.callback=callback
 def open(self,request,timeout):return self.callback(request,timeout)
class Encoder:
 def __init__(self):self.calls=[]
 def encode(self,texts,prompt_name):
  self.calls.append((list(texts),prompt_name))
  return [[float('car' in t or 'vehicle' in t),float('cake' in t or 'baking' in t),0.]for t in texts]
class IndexTests(unittest.TestCase):
 def test_semantic_discovery_cache_change_and_source_scope(self):
  with tempfile.TemporaryDirectory()as d:
   root=pathlib.Path(d);a=root/'a.rs';b=root/'b.py';a.write_text('fn car() {}\n');b.write_text('def cake(): pass\n');enc=Encoder();index=m.Index(root/'cache.sqlite',enc,'test-revision')
   hits=index.search([str(a),str(b)],'vehicle');self.assertEqual(hits[0]['path'],str(a));self.assertEqual(hits[0]['text'],'fn car() {}')
   docs=sum(len(t)for t,p in enc.calls if p=='Document');index.search([str(a),str(b)],'baking');self.assertEqual(sum(len(t)for t,p in enc.calls if p=='Document'),docs)
   a.write_text('fn cake() {}\n');index.search([str(a),str(b)],'vehicle');self.assertEqual(sum(len(t)for t,p in enc.calls if p=='Document'),docs+1)
   self.assertEqual({h['path']for h in index.search([str(b)],'baking')},{str(b)})
   self.assertTrue(all(p in ['Document','CodeRetrieval'] for _,p in enc.calls))
 def test_symlinks_binary_oversized_and_invalid_utf8_are_not_indexed(self):
  with tempfile.TemporaryDirectory()as d:
   r=pathlib.Path(d);a=r/'a.rs';a.write_bytes(b'\xff');b=r/'b.rs';b.write_bytes(b'x\0y');c=r/'c.rs';c.write_text('fn car() {}');s=r/'s.rs';s.symlink_to(c)
   idx=m.Index(r/'cache.sqlite',Encoder(),'test-revision')
   self.assertEqual(idx.search([str(a),str(b),str(s)],'vehicle'),[])
   c.write_bytes(b'x'*(m.MAX_SOURCE+1));self.assertEqual(idx.search([str(c)],'vehicle'),[])
 def test_windows_cover_tail_and_separate_cache_revision(self):
  with tempfile.TemporaryDirectory()as d:
   r=pathlib.Path(d);a=r/'a.py';a.write_text('\n'.join(['# blank']*190+['def car(): pass']))
   enc=Encoder();m.Index(r/'db',enc,'rev1').search([str(a)],'vehicle');n=len(enc.calls)
   hits=m.Index(r/'db',enc,'rev2').search([str(a)],'vehicle');self.assertEqual(hits[0]['text'],'def car(): pass');self.assertGreater(len(enc.calls),n+1)
 def test_corrupt_cached_vector_is_rejected(self):
  with tempfile.TemporaryDirectory()as d:
   root=pathlib.Path(d);source=root/'a.py';source.write_text('def car(): pass')
   index=m.Index(root/'cache.sqlite',Encoder(),'test-revision');index.search([str(source)],'vehicle')
   index.db.execute('UPDATE vectors SET vector=?',(b'\0'*8,));index.db.commit()
   with self.assertRaisesRegex(ValueError,'invalid embedding dimensions'):
    index.search([str(source)],'vehicle')
   index.close()
class OllamaEncoderTests(unittest.TestCase):
 def test_rejects_non_local_api_url(self):
  with self.assertRaisesRegex(ValueError,'must be local'):
   m.OllamaEncoder('https://example.com')
 def test_empty_candidate_set_does_not_start_model(self):
  with tempfile.TemporaryDirectory()as d:
   def fail(request,timeout):raise AssertionError('model must stay unloaded')
   index=m.Index(pathlib.Path(d)/'cache.sqlite',m.OllamaEncoder(opener=Opener(fail)))
   self.assertEqual(index.search([],'anything'),[])
   index.close()
 def test_is_lazy_and_uses_pinned_model_prompts_and_five_minute_keepalive(self):
  calls=[]
  class Response:
   def __init__(self,value):self.value=json.dumps(value).encode()
   def __enter__(self):return self
   def __exit__(self,*args):pass
   def read(self,size=-1):return self.value if size<0 else self.value[:size]
  def open_url(request,timeout):
   calls.append((request,timeout))
   if request.full_url.endswith('/api/tags'):
    return Response({'models':[{'name':m.OLLAMA_MODEL,'digest':m.OLLAMA_DIGEST}]})
   payload=json.loads(request.data)
   return Response({'model':m.OLLAMA_MODEL,'embeddings':[[1.0]+[0.0]*(m.DIMENSION-1) for _ in payload['input']],'load_duration':123 if len(calls)==2 else 0,'total_duration':456})
  encoder=m.OllamaEncoder('http://127.0.0.1:11434',Opener(open_url))
  self.assertEqual(calls,[])
  vectors=encoder.encode(['example.py\ndef car(): pass'],'Document')
  encoder.encode(['car engine'],'CodeRetrieval')
  self.assertEqual(len(vectors[0]),768);self.assertEqual(vectors[0][0],1.0)
  self.assertEqual([r.full_url for r,_ in calls],['http://127.0.0.1:11434/api/tags','http://127.0.0.1:11434/api/embed','http://127.0.0.1:11434/api/embed'])
  request=calls[1][0];payload=json.loads(request.data)
  self.assertEqual(payload['model'],m.OLLAMA_MODEL);self.assertEqual(payload['keep_alive'],'5m')
  self.assertEqual(payload['input'],['title: none | text: example.py\ndef car(): pass'])
  self.assertEqual(json.loads(calls[2][0].data)['input'],['task: code retrieval | query: car engine'])
  self.assertEqual(encoder.stats['load_duration_ns'],123)
 def test_rejects_a_different_local_model_digest(self):
  class Response:
   def __enter__(self):return self
   def __exit__(self,*args):pass
   def read(self,size=-1):
    value=json.dumps({'models':[{'name':m.OLLAMA_MODEL,'digest':'wrong'}]}).encode()
    return value if size<0 else value[:size]
  encoder=m.OllamaEncoder('http://127.0.0.1:11434',Opener(lambda request,timeout:Response()))
  with self.assertRaisesRegex(ValueError,'pinned Ollama model'):
   encoder.encode(['x'],'CodeRetrieval')
 def test_rejects_non_768_dimension_response(self):
  class Response:
   def __enter__(self):return self
   def __exit__(self,*args):pass
   def read(self,size=-1):
    value=json.dumps({'model':m.OLLAMA_MODEL,'embeddings':[[1.0]*512]}).encode()
    return value if size<0 else value[:size]
  encoder=m.OllamaEncoder('http://127.0.0.1:11434',Opener(lambda request,timeout:Response()))
  encoder.verified=True
  with self.assertRaisesRegex(ValueError,'768'):
   encoder.encode(['x'],'CodeRetrieval')
if __name__=='__main__':unittest.main()
