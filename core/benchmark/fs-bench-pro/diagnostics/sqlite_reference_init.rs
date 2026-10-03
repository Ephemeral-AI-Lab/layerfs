//! Reference-only harness for the unmodified pinned Phase 4.5 Service import.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer,contract::*};
use layerfs_history::{sqlite,HistoryCatalogConfig};
use layerfs_server::{Service,StoreAccess,Grant};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder,timer::Timing};
use std::{fmt::Write as _,path::Path,sync::Arc,time::{Duration,Instant},io::Cursor};
fn hex(bytes:&[u8])->String {let mut s=String::new();for b in bytes {write!(s,"{b:02x}").unwrap();}s}
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args=std::env::args().collect::<Vec<_>>();if args.len()!=5 {return Err("source store history name required".into());}
 let start=Instant::now();
 let store=Timing::disabled("create",|t|Store::create(&args[2],Store::default_policy(),t.child("store"))).0?;
 let history=sqlite::create(Path::new(&args[3]),&HistoryCatalogConfig{binding_key:b"layerfs-bench-pro".to_vec(),incarnation:1,cursor_key:[0x28;32]})?;
 let peer=VerifiedPeer::from_private(&[0x39;32])?;
 let mut service=Service::new(vec![StoreAccess{id:1,store,history:Some(Arc::new(history)),grants:vec![Grant{public_key:*peer.public_key(),operations:u8::MAX,expires_unix:u64::MAX}]}],OperationRecorder::disabled())?;
 let bootstrap_ns=start.elapsed().as_nanos();
 let request=Request{id:1,generation:1,store:1,profile:HISTORY_PROFILE,deadline_ms:MAX_OPERATION_MS,response_bytes:u64::from(MAX_OPERATION_MS.div_ceil(1000)),operation:Operation::HistoryCommand(HistoryCommand::ImportNativeDirectory{stack:[0x41;16],name:args[4].as_bytes().to_vec(),scope_seed:[0x42;32]})};
 let init_start=Instant::now();service.set_import_root(Path::new(&args[1]))?;
 let (result,_)=service.handle_until(&peer,&request,&mut Cursor::new([]),&mut Vec::new(),start+Duration::from_secs(15));let init_ns=init_start.elapsed().as_nanos();
 let created=match result?{Response::History(value)=>match *value{HistoryResult::StackCreated(value)=>value,_=>return Err("wrong history response".into())},_=>return Err("wrong response".into())};
 let close=Instant::now();drop(service);let close_ns=close.elapsed().as_nanos();let complete_product_ns=start.elapsed().as_nanos();
 println!("{{\"status\":\"COMPLETE\",\"operation_ns\":{complete_product_ns},\"bootstrap_ns\":{bootstrap_ns},\"init_ns\":{init_ns},\"close_ns\":{close_ns},\"root\":\"{}\",\"stack\":\"{}\",\"root_serial\":{}}}",hex(&created.root),hex(&[0x41;16]),created.root_serial);
 Ok(())
}
