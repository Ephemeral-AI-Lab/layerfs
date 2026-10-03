//! Diagnostic public Service import of unmodified Phase 4.5 components.
#[path="support/diagnostic.rs"] mod diag;
use layerfs_bridge::{adapters::native::connection::VerifiedPeer,contract::*};
use layerfs_history::{sqlite,HistoryCatalogConfig};
use layerfs_server::{Service,StoreAccess,Grant};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::{OperationRecorder,Diagnostic},timer::{Timing,RecordingLimits}};
use std::{path::Path,fs::File,sync::Arc,time::{Duration,Instant},io::Cursor};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args=std::env::args().collect::<Vec<_>>();if args.len()!=5{return Err("source store history report-dir required".into())}
 let folder=Path::new(&args[4]);let cursor=diag::unhex(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?;
 let mut phases=diag::Phases::new();std::env::set_var("LAYERFS_SQLITE_SCOPE","setup");
 let store=phases.measure("setup.store",||Timing::disabled("create",|t|Store::create(&args[2],Store::default_policy(),t.child("store"))).0)?;
 let history=phases.measure("setup.history",||sqlite::create(Path::new(&args[3]),&HistoryCatalogConfig{binding_key:b"layerfs-bench-pro".to_vec(),incarnation:1,cursor_key:cursor}))?;
 let peer=VerifiedPeer::from_private(&[0x39;32])?;
 let recorder=OperationRecorder::new(RecordingLimits::new(1024,32,256*1024).unwrap(),1,4*1024*1024).unwrap();
 let mut service=Service::new(vec![StoreAccess{id:1,store,history:Some(Arc::new(history)),grants:vec![Grant{public_key:*peer.public_key(),operations:u8::MAX,expires_unix:u64::MAX}]}],recorder)?;
 let request=Request{id:1,generation:1,store:1,profile:HISTORY_PROFILE,deadline_ms:MAX_OPERATION_MS,response_bytes:u64::from(MAX_OPERATION_MS.div_ceil(1000)),operation:Operation::HistoryCommand(HistoryCommand::ImportNativeDirectory{stack:[0x41;16],name:b"diagnostic-init".to_vec(),scope_seed:[0x42;32]})};
 std::env::set_var("LAYERFS_SQLITE_SCOPE","operation");let cpu=diag::cpu_ns();let start=Instant::now();
 service.set_import_root(Path::new(&args[1]))?;
 let (result,detail)=service.handle_until(&peer,&request,&mut Cursor::new([]),&mut Vec::new(),start+Duration::from_secs(15));
 let operation_ns=start.elapsed().as_nanos();let cpu_ns=diag::cpu_ns()-cpu;
 let created=match result?{Response::History(value)=>match *value{HistoryResult::StackCreated(value)=>value,_=>return Err("wrong history response".into())},_=>return Err("wrong response".into())};
 match detail{Diagnostic::Report(report)=>{report.timing().write_json(File::create(folder.join("timing.json"))?)?;},_=>return Err("timing report unavailable".into())}
 std::env::set_var("LAYERFS_SQLITE_SCOPE","cleanup");
 println!("{{\"status\":\"PASS\",\"kind\":\"CAUSE_DIAGNOSTIC\",\"operation_ns\":{operation_ns},\"cpu_ns\":{cpu_ns},\"root\":\"{}\",\"stack\":\"{}\",\"phases\":{}}}",diag::hex(&created.root),diag::hex(&[0x41;16]),phases.json());
 Ok(())
}
