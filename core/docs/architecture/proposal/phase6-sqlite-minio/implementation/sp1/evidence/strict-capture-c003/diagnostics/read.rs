use std::sync::{Arc,Mutex};
use layerfs_content::{AuthenticatedObjects,ObjectId};
use layerfs_storage::{StorageCapacities,StoragePolicy};
use phase6_live_probe::{minio::{Minio,digest},strict_catalog::{StrictCatalog,LogicalUse},strict_read::WaveReader};
fn main(){
 let root=std::env::current_dir().unwrap();
 let catalog=Arc::new(StrictCatalog::open_read_only(&root.join("benchmark-results/sp1-strict-provider/r001/commit-c003/databases/strict-commit-catalog.sqlite")).unwrap());
 let provider=Minio{authority:std::env::var("SP1_MINIO_AUTHORITY").unwrap(),bucket:"sp1-r001-commit-c003".into(),access:std::env::var("SP1_MINIO_ACCESS").unwrap(),secret:std::env::var("SP1_MINIO_SECRET").unwrap(),stats:Arc::new(Mutex::new(Default::default()))};
 let view=WaveReader{catalog:catalog.clone(),provider:provider.clone(),scope:catalog.capture(Some(3)).unwrap(),logical_use:LogicalUse::RegularFileGraph,capacities:StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap()};
 let raw=(0..32).map(|i|u8::from_str_radix(&"62f38d975e15e1a3f398bc36af9f8994f19f67632dc329cb996fd3c62f0c3a14"[i*2..i*2+2],16).unwrap()).collect::<Vec<_>>();
 let id=ObjectId::from_bytes(&raw).unwrap();
 let canonical=view.read_canonical(id).unwrap();
 println!("actual chunked FileState canonical {:?}",canonical);
 let mut bytes=Vec::new();
 layerfs_telemetry::timer::Timing::disabled("diagnostic",|scope|layerfs_content::read_all(&view,id,&mut bytes,scope.child("read"))).0.unwrap();
 let expected=std::fs::read(root.join("core/docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/evidence/strict-oracle-v2/vectors/state1-chunked.raw")).unwrap();
 let diffs=bytes.iter().zip(&expected).enumerate().filter(|(_, (a,b))|a!=b).map(|(i,_)|i).collect::<Vec<_>>();
 println!("DIAGNOSTIC actual_len={} expected_len={} mismatch_count={} first={:?} actual_sha256={:?} expected_sha256={:?} provider={:?}",bytes.len(),expected.len(),diffs.len(),diffs.first(),digest(&bytes),digest(&expected),provider.stats.lock().unwrap());
 if let Some(i)=diffs.first(){println!("mismatch actual={} expected={}",bytes[*i],expected[*i]);}
}
