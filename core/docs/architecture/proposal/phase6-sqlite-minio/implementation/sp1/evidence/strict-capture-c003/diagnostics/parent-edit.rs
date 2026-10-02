#[path="/Users/yifanxu/.codex/worktrees/phase6-sp1-strict/layerfs/target/sp1-oracle-source/core/crates/layerfs-content/tests/support/mod.rs"]
mod support;
use layerfs_content::{FinalizedConsumer,FinalizedObject,ObjectRole,ObjectId,ConstructionPolicy,Edit,EditRequest};
fn main(){
 let r=std::env::current_dir().unwrap();
 let v=r.join("core/docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/evidence/strict-oracle-v2/vectors");
 let mut store=support::MemoryStore::new();
 for line in std::fs::read_to_string(v.join("state0-chunked.tsv")).unwrap().lines().skip(1){let c=line.split('\t').collect::<Vec<_>>();let canonical=std::fs::read(v.join(format!("{}.canonical",c[1]))).unwrap();store.accept(FinalizedObject::new(ObjectRole::from_code(c[2].parse().unwrap()).unwrap(),canonical).unwrap()).unwrap();}
 let raw=(0..32).map(|i|u8::from_str_radix(&"305406a42d5a606fe0824693d19d8f860284026320a02b9b97f84bea559f65a2"[i*2..i*2+2],16).unwrap()).collect::<Vec<_>>();let root=ObjectId::from_bytes(&raw).unwrap();
 let sequence=support::edits::Edits::new(200000,vec![Edit::overwrite(64,96)]).unwrap();
 let expected=std::fs::read(v.join("state1-chunked.raw")).unwrap();let mut parts=support::edits::Parts::new();parts.push(expected[64..96].to_vec());
 let mut emitted=support::MemoryStore::new();let policy=ConstructionPolicy::frozen_default();
 let edited=layerfs_telemetry::timer::Timing::disabled("baseline",|scope|layerfs_content::apply_edits(policy,&policy.capacities(),&store,EditRequest{root,edits:&sequence,source:&parts},&mut emitted,scope.child("edit"))).0.unwrap();
 println!("DIAGNOSTIC unchanged-parent apply_edits root={:?} logical_len={} old-complete-stream-root=ccead465af722b1f8f26e4fa8dc4db5ac586a0bd51c3f589320d246664b19506; canonical={:?}",edited.root,edited.logical_len,emitted.canonical(edited.root));
}
