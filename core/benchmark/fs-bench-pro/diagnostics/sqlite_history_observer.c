/* First-party fixed-count qualification observer; original SQLite/VFS operations. */
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
void cause_history_note_pack(sqlite3_stmt*statement){
 const char*sql=sqlite3_sql(statement);
 if(!sql||(strncmp(sql,"SELECT body FROM pack ",strlen("SELECT body FROM pack "))&&strncmp(sql,"SELECT p.data FROM object_packs ",strlen("SELECT p.data FROM object_packs "))))return;
 atomic_fetch_add(&pack_reads,1);char*text=sqlite3_expanded_sql(statement);
 if(!text){atomic_fetch_add(&pack_unknown,1);return;}
 const char*where=strstr(text,"WHERE");const char*key=where?strstr(where,"pack_id"):0;const char*equals=key?strchr(key,'='):0;
 char*end=0;long long id=equals?strtoll(equals+1,&end,10):0;
 if(!equals||end==equals+1||id<=0||id>=65536)atomic_fetch_add(&pack_unknown,1);
 else{uint64_t bit=UINT64_C(1)<<(id%64);uint64_t old=atomic_fetch_or(&pack_seen[id/64],bit);if(!(old&bit))atomic_fetch_add(&distinct_packs,1);}
 sqlite3_free(text);
}
void cause_history_snapshot(uint64_t*out){
 for(int i=0;i<20;i++)out[i]=0;
 pthread_mutex_lock(&guard);
 for(size_t i=0;i<used;i++){out[0]+=rows[i].calls;out[1]+=rows[i].steps;out[2]+=rows[i].step_ns;out[3]+=rows[i].exec_ns;out[4]+=rows[i].scans;out[5]+=rows[i].sorts;out[6]+=rows[i].reprepare;if(!strcmp(rows[i].sql,"BEGIN"))out[18]+=rows[i].calls;}
 pthread_mutex_unlock(&guard);
 out[7]=atomic_load(&prepare_calls);out[8]=atomic_load(&prepare_ns);out[9]=atomic_load(&reset_calls);out[10]=atomic_load(&reset_ns);
 for(int i=0;i<4;i++){out[11]+=atomic_load(&read_bytes[i]);out[12]+=atomic_load(&write_bytes[i]);out[13]+=atomic_load(&syncs[i]);out[14]+=atomic_load(&closes[i]);}
 out[15]=atomic_load(&pack_reads);out[16]=atomic_load(&distinct_packs);out[17]=atomic_load(&pack_unknown);out[19]=1;
}
