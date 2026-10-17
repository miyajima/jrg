import importlib.util,pathlib,tempfile,unittest
spec=importlib.util.spec_from_file_location('embedding',pathlib.Path(__file__).with_name('embedding.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
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
if __name__=='__main__':unittest.main()
