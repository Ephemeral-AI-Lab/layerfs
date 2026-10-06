/* First-party count diagnostic over public SQLite APIs and the existing
 * exact-delegation VFS. No trace callback or stmt-status reset is installed.
 * Fixed arrays cover the declared Durable100 scope; omitted events invalidate
 * the observation but do not stop or limit product work. */
#include "sqlite_vfs_observer.c"
#define INIT_EVENTS 128
#define SNAPSHOT_WORDS 32
static _Thread_local char unit_name[48],last_sql[200];
static _Thread_local uint64_t unit_id;
struct InitEvent { uint64_t id,wall_ns,before[SNAPSHOT_WORDS],after[SNAPSHOT_WORDS];int scope,result;char unit[48],previous_sql[200]; };
static struct InitEvent init_events[INIT_EVENTS];
static _Atomic unsigned event_count,event_omitted;
int cause_init_vfs_snapshot(uint64_t *out,size_t count){
 if(!out||count!=SNAPSHOT_WORDS)return SQLITE_MISUSE;
 for(int k=0;k<4;k++){
  out[k*8]=atomic_load(&reads[k]);out[k*8+1]=atomic_load(&read_bytes[k]);out[k*8+2]=atomic_load(&read_ns[k]);
  out[k*8+3]=atomic_load(&writes[k]);out[k*8+4]=atomic_load(&write_bytes[k]);out[k*8+5]=atomic_load(&write_ns[k]);
  out[k*8+6]=atomic_load(&syncs[k]);out[k*8+7]=atomic_load(&sync_ns[k]);
 }
 return SQLITE_OK;
}
int cause_init_vfs_unit(const char *name,uint64_t id){
 if(!name||strlen(name)>=sizeof(unit_name))return SQLITE_MISUSE;
 strcpy(unit_name,name);unit_id=id;return SQLITE_OK;
}
static int init_observed_step(sqlite3_stmt *statement){
 const char *sql=sqlite3_sql(statement);
 int selected=sql&&!strcmp(sql,"COMMIT")?1:sql&&strstr(sql,"wal_checkpoint")?2:0;
 struct InitEvent *row=0;
 if(selected){unsigned at=atomic_fetch_add(&event_count,1);if(at<INIT_EVENTS){
  row=&init_events[at];row->id=unit_id;row->scope=selected;
  strcpy(row->unit,unit_name);strcpy(row->previous_sql,last_sql);
  cause_init_vfs_snapshot(row->before,SNAPSHOT_WORDS);
 }else atomic_fetch_add(&event_omitted,1);}
 int previous=cause_sql_scope;cause_sql_scope=selected;
 uint64_t start=now();int result=sqlite3_step(statement);uint64_t elapsed=now()-start;
 cause_sql_scope=previous;
 if(row){row->result=result;row->wall_ns=elapsed;cause_init_vfs_snapshot(row->after,SNAPSHOT_WORDS);}
 if(!selected&&sql)snprintf(last_sql,sizeof(last_sql),"%s",sql);
 return result;
}
struct InitHook {const void *replacement,*original;};
__attribute__((used,section("__DATA,__interpose,interposing")))
static const struct InitHook init_hook={(const void*)&init_observed_step,(const void*)&sqlite3_step};
static void init_quote(FILE *f,const char *s){
 fputc('"',f);for(const unsigned char*p=(const unsigned char*)s;*p;p++){
  if(*p=='"'||*p=='\\')fprintf(f,"\\%c",*p);else if(*p<32)fprintf(f,"\\u%04x",*p);else fputc(*p,f);
 }fputc('"',f);
}
__attribute__((destructor))static void init_report(void){
 const char *path=getenv("LAYERFS_INIT_VFS_EVENTS");if(!path)return;
 FILE *f=fopen(path,"wx");if(!f){fprintf(stderr,"Init VFS event output refused\n");return;}
 fprintf(f,"{\"kind\":\"init-vfs-port-and-step-diagnostic\",\"event_limit\":%d,\"events_attempted\":%u,\"omitted\":%u,\"native_stmt_status_reset\":false,\"events\":[",INIT_EVENTS,atomic_load(&event_count),atomic_load(&event_omitted));
 unsigned total=atomic_load(&event_count);if(total>INIT_EVENTS)total=INIT_EVENTS;
 for(unsigned i=0;i<total;i++){struct InitEvent*r=&init_events[i];if(i)fputc(',',f);
  fprintf(f,"{\"unit_id\":%llu,\"unit\":",(unsigned long long)r->id);init_quote(f,r->unit);
  fprintf(f,",\"previous_sql\":");init_quote(f,r->previous_sql);
  fprintf(f,",\"scope\":%d,\"result\":%d,\"step_ns\":%llu,\"classes\":[",r->scope,r->result,(unsigned long long)r->wall_ns);
  for(int k=0;k<4;k++){if(k)fputc(',',f);fputc('[',f);for(int j=0;j<8;j++){if(j)fputc(',',f);fprintf(f,"%llu",(unsigned long long)(r->after[k*8+j]-r->before[k*8+j]));}fputc(']',f);}
  fprintf(f,"]}");
 }fprintf(f,"]}\n");if(fclose(f))fprintf(stderr,"Init VFS event close failed\n");
}
