/* External first-party SQLite observer. No changes to SQLite or product source. */
#include <sqlite3.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifndef CLASSES
#define CLASSES 512
#endif
#define SQL_TEXT 512
struct row { uint64_t id,calls,steps,scans,sorts,reprepare,profile_ns,step_calls,step_ns,exec_calls,exec_ns; int phase,owner; char sql[SQL_TEXT]; };
static struct row rows[CLASSES];
static size_t used;
#define CLASS_SLOTS (CLASSES * 2)
/* Counter-key navigation only: no object/result data, fixed32KiB at4096classes. */
static uint32_t class_slots[CLASS_SLOTS];
static uint64_t class_lookup_calls,class_lookup_probes;
static uint64_t omitted, opens;
static pthread_mutex_t guard=PTHREAD_MUTEX_INITIALIZER;
static uint64_t now_ns(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return (uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec; }
static int phase(void) { const char *p=getenv("LAYERFS_SQLITE_SCOPE"); return p && !strcmp(p,"operation") ? 1 : p && !strcmp(p,"cleanup") ? 2 : 0; }
static int owner(sqlite3_stmt *s) { const char *p=sqlite3_db_filename(sqlite3_db_handle(s),"main"); return p && strstr(p,"history.sqlite") ? 5 : p && strstr(p,"store.sqlite") ? 2 : 0; }
static struct row *get_sql(const char *sql,int ow) {
 if(!sql) sql="";
 uint64_t h=UINT64_C(14695981039346656037); for(const unsigned char *p=(const unsigned char*)sql;*p;p++) h=(h^*p)*UINT64_C(1099511628211);
 int ph=phase();
 size_t slot=(size_t)((h ^ (uint64_t)ow*UINT64_C(0x9e3779b97f4a7c15) ^ (uint64_t)ph*UINT64_C(0x517cc1b727220a95)) % CLASS_SLOTS);
 class_lookup_calls++;
 for(size_t searched=0;searched<CLASS_SLOTS;searched++){
  class_lookup_probes++;uint32_t index=class_slots[slot];
  if(index){struct row *r=&rows[index-1];if(r->id==h&&r->phase==ph&&r->owner==ow)return r;}
  else{
   if(used==CLASSES){omitted++;return NULL;}
   struct row *r=&rows[used];class_slots[slot]=(uint32_t)++used;
   r->id=h;r->phase=ph;r->owner=ow;snprintf(r->sql,SQL_TEXT,"%s",sql);return r;
  }
  slot=(slot+1)%CLASS_SLOTS;
 }
 omitted++;return NULL;
}
static struct row *get(sqlite3_stmt *s) { return get_sql(sqlite3_sql(s),owner(s)); }
#ifdef LAYERFS_COMBINED_OBSERVER
void cause_history_note_pack(sqlite3_stmt *statement);
#endif
static int profile(unsigned event,void *ctx,void *statement,void *time) {
 (void)ctx; if(event!=SQLITE_TRACE_PROFILE) return 0;
 sqlite3_stmt *s=statement;
#ifdef LAYERFS_COMBINED_OBSERVER
 cause_history_note_pack(s);
#endif
 pthread_mutex_lock(&guard);struct row *r=get(s);
 if(r) {r->calls++;r->steps+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_VM_STEP,1);r->scans+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_FULLSCAN_STEP,1);r->sorts+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_SORT,1);r->reprepare+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_REPREPARE,1);r->profile_ns+=*(sqlite3_uint64*)time;}
 pthread_mutex_unlock(&guard);return 0;
}
void cause_sqlite_trace_attach(sqlite3 *db) {
 sqlite3_trace_v2(db,SQLITE_TRACE_PROFILE,profile,NULL);pthread_mutex_lock(&guard);opens++;pthread_mutex_unlock(&guard);
}
#ifndef LAYERFS_COMBINED_OBSERVER
static int observed_open(const char *name,sqlite3 **out,int flags,const char *vfs) {
 int result=sqlite3_open_v2(name,out,flags,vfs);
 if(result==SQLITE_OK) cause_sqlite_trace_attach(*out);
 return result;
}
#endif
#ifndef LAYERFS_SQL_CALL_SCOPE
#define LAYERFS_SQL_CALL_SCOPE 1
static _Thread_local int cause_sql_scope;
#endif
static int observed_step(sqlite3_stmt *s) {
 const char *sql=sqlite3_sql(s);int previous=cause_sql_scope;
 cause_sql_scope=sql&&!strcmp(sql,"COMMIT")?1:sql&&strstr(sql,"wal_checkpoint")?2:0;
 uint64_t start=now_ns();int result=sqlite3_step(s);uint64_t duration=now_ns()-start;cause_sql_scope=previous;
 pthread_mutex_lock(&guard);struct row *r=get(s);if(r){r->step_calls++;r->step_ns+=duration;}pthread_mutex_unlock(&guard);return result;
}
#define INTERPOSE(replacement, original) __attribute__((used)) static const struct { const void *replace;const void *replacee; } replacement##_binding __attribute__((section("__DATA,__interpose,interposing"))) = { (const void*)(uintptr_t)&replacement,(const void*)(uintptr_t)&original };

#ifndef LAYERFS_COMBINED_OBSERVER
INTERPOSE(observed_open,sqlite3_open_v2)
#endif
INTERPOSE(observed_step,sqlite3_step)
static int observed_exec(sqlite3 *db,const char *sql,int (*callback)(void*,int,char**,char**),void *ctx,char **error) {
 uint64_t start=now_ns();int result=sqlite3_exec(db,sql,callback,ctx,error);uint64_t duration=now_ns()-start;
 const char *path=sqlite3_db_filename(db,"main");int ow=path && strstr(path,"history.sqlite")?5:path && strstr(path,"store.sqlite")?2:0;
 pthread_mutex_lock(&guard);struct row *r=get_sql(sql,ow);if(r){r->exec_calls++;r->exec_ns+=duration;}pthread_mutex_unlock(&guard);return result;
}
INTERPOSE(observed_exec,sqlite3_exec)

static void quoted(FILE *f,const char *s) {fputc('"',f);for(const unsigned char *p=(const unsigned char*)s;*p;p++){if(*p=='"'||*p=='\\')fprintf(f,"\\%c",*p);else if(*p<32)fprintf(f,"\\u%04x",*p);else fputc(*p,f);}fputc('"',f);}
static void finish(void) {
 const char *path=getenv("LAYERFS_SQLITE_WORK_OUTPUT");if(!path) return;
 FILE *f=fopen(path,"wx");if(!f){perror("SQLite observer output");return;}
 fprintf(f,"{\"schema\":\"sqlite-work-v1\",\"opens\":%llu,\"omitted\":%llu,\"classes\":[",(unsigned long long)opens,(unsigned long long)omitted);
 for(size_t i=0;i<used;i++){struct row *r=&rows[i];if(i)fputc(',',f);fprintf(f,"{\"sql_id\":\"%016llx\",\"phase\":%d,\"owner\":%d,\"calls\":%llu,\"vm_steps\":%llu,\"fullscan_steps\":%llu,\"sorts\":%llu,\"reprepare\":%llu,\"profile_ns\":%llu,\"step_calls\":%llu,\"step_ns\":%llu,\"exec_calls\":%llu,\"exec_ns\":%llu,\"sql_prefix\":",(unsigned long long)r->id,r->phase,r->owner,(unsigned long long)r->calls,(unsigned long long)r->steps,(unsigned long long)r->scans,(unsigned long long)r->sorts,(unsigned long long)r->reprepare,(unsigned long long)r->profile_ns,(unsigned long long)r->step_calls,(unsigned long long)r->step_ns,(unsigned long long)r->exec_calls,(unsigned long long)r->exec_ns);quoted(f,r->sql);fputc('}',f);}
 fprintf(f,"],\"class_lookup\":{\"kind\":\"fixed-index-over-existing-counter-rows\",\"slots\":%d,\"index_bytes\":%zu,\"calls\":%llu,\"probes\":%llu}}\n",CLASS_SLOTS,sizeof(class_slots),(unsigned long long)class_lookup_calls,(unsigned long long)class_lookup_probes);fclose(f);
}

__attribute__((constructor)) static void register_output(void) { atexit(finish); }
