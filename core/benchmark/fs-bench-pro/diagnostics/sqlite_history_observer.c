/* First-party fixed-count qualification observer; original SQLite/VFS operations. */
#include <execinfo.h>
#include <dlfcn.h>
#define LAYERFS_COMBINED_OBSERVER 1
#define CLASSES 4096
#define opens trace_opens
#include "sqlite_work.c"
#undef opens
#include "sqlite_close_observer.c"
/* Fixed snapshot differences attribute actual work without changing SQL scope. */
static _Atomic unsigned long long prepare_calls,prepare_ns,reset_calls,reset_ns;
static int history_prepare_v2(sqlite3*d,const char*s,int n,sqlite3_stmt**out,const char**tail){uint64_t t=now();int r=sqlite3_prepare_v2(d,s,n,out,tail);atomic_fetch_add(&prepare_calls,1);atomic_fetch_add(&prepare_ns,now()-t);return r;}
static int history_prepare_v3(sqlite3*d,const char*s,int n,unsigned flags,sqlite3_stmt**out,const char**tail){uint64_t t=now();int r=sqlite3_prepare_v3(d,s,n,flags,out,tail);atomic_fetch_add(&prepare_calls,1);atomic_fetch_add(&prepare_ns,now()-t);return r;}
static int history_reset(sqlite3_stmt*s){uint64_t t=now();int r=sqlite3_reset(s);atomic_fetch_add(&reset_calls,1);atomic_fetch_add(&reset_ns,now()-t);return r;}
INTERPOSE(history_prepare_v2,sqlite3_prepare_v2)
INTERPOSE(history_prepare_v3,sqlite3_prepare_v3)
INTERPOSE(history_reset,sqlite3_reset)
static _Atomic uint64_t pack_seen[1024];
static _Atomic uint64_t pack_reads,distinct_packs,pack_unknown;
/* SQL body reads and successful read-only BLOB opens are acquisitions; an open
 * identifies the pack, while the product's explicit counters count BLOB bytes.
 * Fixed bitmap attribution is shared across both routes, without a body read. */
static void history_note_pack_id(sqlite3_int64 id){
 atomic_fetch_add(&pack_reads,1);
 if(id<=0||id>=65536)atomic_fetch_add(&pack_unknown,1);
 else{uint64_t bit=UINT64_C(1)<<(id%64);uint64_t old=atomic_fetch_or(&pack_seen[id/64],bit);if(!(old&bit))atomic_fetch_add(&distinct_packs,1);}
}
/* These counters observe delegated SQLite calls. Read/close cover all BLOB
 * handles; matched opens cover read-only main.pack.body. No payload inspection. */
static _Atomic uint64_t blob_work[11];
static _Atomic uint64_t pack_materialized_bytes;
/* Optional bounded external acquisition trace. No product bytes inspected. */
static FILE *acquisition_trace;
static int acquisition_trace_initialized;
static sqlite3_blob *trace_handles[64];
static sqlite3_int64 trace_ids[64];
static uint64_t trace_sequence;
static uint64_t trace_vfs_bytes(void){
 uint64_t bytes=0;for(int i=0;i<4;i++)bytes+=atomic_load(&read_bytes[i]);return bytes;
}
static void trace_event(const char *event,sqlite3_int64 id,int offset,int bytes,int result,uint64_t before){
 if(!acquisition_trace_initialized){
  acquisition_trace_initialized=1;const char *path=getenv("LAYERFS_ACQUISITION_TRACE");
  if(path){acquisition_trace=fopen(path,"wx");if(!acquisition_trace){fprintf(stderr,"acquisition trace open refused\n");abort();}}
 }
 if(acquisition_trace&&fprintf(acquisition_trace,"{\"seq\":%llu,\"event\":\"%s\",\"pack\":%lld,\"offset\":%d,\"bytes\":%d,\"result\":%d,\"vfs_before\":%llu,\"vfs_after\":%llu}\n",(unsigned long long)++trace_sequence,event,(long long)id,offset,bytes,result,(unsigned long long)before,(unsigned long long)trace_vfs_bytes())<0)abort();
}
static void trace_stack(void){
 if(!acquisition_trace)return;
 void *frames[32];int count=backtrace(frames,32);Dl_info image;
 if(fprintf(acquisition_trace,"{\"seq\":%llu,\"event\":\"stack\",\"frames\":[",(unsigned long long)++trace_sequence)<0)abort();
 for(int i=0;i<count;i++){
  if(i&&fputc(',',acquisition_trace)==EOF)abort();
  if(dladdr(frames[i],&image)&&image.dli_fname&&strstr(image.dli_fname,"benchmark_history")){
   if(fprintf(acquisition_trace,"%llu",(unsigned long long)((uintptr_t)frames[i]-(uintptr_t)image.dli_fbase))<0)abort();
  }else if(fputs("null",acquisition_trace)==EOF)abort();
 }
 if(fputs("]}\n",acquisition_trace)==EOF)abort();
}
static int trace_slot(sqlite3_blob *blob){for(int i=0;i<64;i++)if(trace_handles[i]==blob)return i;return -1;}
__attribute__((destructor)) static void trace_finish(void){if(acquisition_trace&&fclose(acquisition_trace))abort();}
static int history_blob_open(sqlite3*db,const char*database,const char*table,const char*column,sqlite3_int64 row,int writable,sqlite3_blob**out){
 int matched=!writable&&database&&table&&column&&!strcmp(database,"main")&&!strcmp(table,"pack")&&!strcmp(column,"body");
 uint64_t before=trace_vfs_bytes();uint64_t start=now();int result=sqlite3_blob_open(db,database,table,column,row,writable,out);
 if(matched){atomic_fetch_add(&blob_work[0],1);atomic_fetch_add(&blob_work[1],now()-start);if(result!=SQLITE_OK)atomic_fetch_add(&blob_work[2],1);else history_note_pack_id(row);}
 if(matched){trace_event("open",row,0,0,result,before);trace_stack();if(result==SQLITE_OK){int slot=trace_slot(NULL);if(slot<0)abort();trace_handles[slot]=*out;trace_ids[slot]=row;}}
 return result;
}
static int history_blob_read(sqlite3_blob*blob,void*buffer,int bytes,int offset){
 uint64_t before=trace_vfs_bytes();int slot=trace_slot(blob);uint64_t start=now();int result=sqlite3_blob_read(blob,buffer,bytes,offset);
 atomic_fetch_add(&blob_work[3],1);if(bytes>0)atomic_fetch_add(&blob_work[4],(uint64_t)bytes);
 atomic_fetch_add(&blob_work[6],now()-start);
 if(result==SQLITE_OK){if(bytes>0){atomic_fetch_add(&blob_work[5],(uint64_t)bytes);atomic_fetch_add(&pack_materialized_bytes,(uint64_t)bytes);}}else atomic_fetch_add(&blob_work[7],1);
 if(slot>=0)trace_event("read",trace_ids[slot],offset,bytes,result,before);
 return result;
}
static int history_blob_close(sqlite3_blob*blob){
 uint64_t before=trace_vfs_bytes();int slot=blob?trace_slot(blob):-1;uint64_t start=now();int result=sqlite3_blob_close(blob);
 atomic_fetch_add(&blob_work[8],1);atomic_fetch_add(&blob_work[9],now()-start);if(result!=SQLITE_OK)atomic_fetch_add(&blob_work[10],1);
 if(slot>=0){trace_event("close",trace_ids[slot],0,0,result,before);trace_handles[slot]=NULL;}
 return result;
}
INTERPOSE(history_blob_open,sqlite3_blob_open)
INTERPOSE(history_blob_read,sqlite3_blob_read)
INTERPOSE(history_blob_close,sqlite3_blob_close)
void cause_history_blob_snapshot(uint64_t*out){
 for(int i=0;i<11;i++)out[i]=atomic_load(&blob_work[i]);out[11]=1;
}
/* Observe the original body extraction, without pre-reading or copying bytes. */
static const void* history_column_blob(sqlite3_stmt*statement,int column){
 const void*body=sqlite3_column_blob(statement,column);
 const char*sql=sqlite3_sql(statement);
 if(body&&column==0&&sql&&(!strncmp(sql,"SELECT body FROM pack ",strlen("SELECT body FROM pack "))||!strncmp(sql,"SELECT p.data FROM object_packs ",strlen("SELECT p.data FROM object_packs ")))){
  int bytes=sqlite3_column_bytes(statement,column);if(bytes>0)atomic_fetch_add(&pack_materialized_bytes,(uint64_t)bytes);
 }
 return body;
}
INTERPOSE(history_column_blob,sqlite3_column_blob)
void cause_history_acquired_snapshot(uint64_t*out){out[0]=atomic_load(&pack_materialized_bytes);out[1]=1;}
void cause_history_note_pack(sqlite3_stmt*statement){
 const char*sql=sqlite3_sql(statement);
 if(!sql||(strncmp(sql,"SELECT body FROM pack ",strlen("SELECT body FROM pack "))&&strncmp(sql,"SELECT p.data FROM object_packs ",strlen("SELECT p.data FROM object_packs "))))return;
 char*text=sqlite3_expanded_sql(statement);
 if(!text){history_note_pack_id(0);return;}
 const char*where=strstr(text,"WHERE");const char*key=where?strstr(where,"pack_id"):0;const char*equals=key?strchr(key,'='):0;
 char*end=0;long long id=equals?strtoll(equals+1,&end,10):0;
 history_note_pack_id(!equals||end==equals+1?0:id);
 sqlite3_free(text);
}
void cause_history_snapshot(uint64_t*out){
 trace_event("snapshot",0,0,0,0,trace_vfs_bytes());
 for(int i=0;i<20;i++)out[i]=0;
 pthread_mutex_lock(&guard);
 for(size_t i=0;i<used;i++){out[0]+=rows[i].calls;out[1]+=rows[i].steps;out[2]+=rows[i].step_ns;out[3]+=rows[i].exec_ns;out[4]+=rows[i].scans;out[5]+=rows[i].sorts;out[6]+=rows[i].reprepare;if(!strcmp(rows[i].sql,"BEGIN"))out[18]+=rows[i].calls;}
 pthread_mutex_unlock(&guard);
 out[7]=atomic_load(&prepare_calls);out[8]=atomic_load(&prepare_ns);out[9]=atomic_load(&reset_calls);out[10]=atomic_load(&reset_ns);
 for(int i=0;i<4;i++){out[11]+=atomic_load(&read_bytes[i]);out[12]+=atomic_load(&write_bytes[i]);out[13]+=atomic_load(&syncs[i]);out[14]+=atomic_load(&closes[i]);}
 out[15]=atomic_load(&pack_reads);out[16]=atomic_load(&distinct_packs);out[17]=atomic_load(&pack_unknown);out[19]=1;
}
