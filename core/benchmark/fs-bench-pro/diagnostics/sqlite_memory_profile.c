/* Diagnostic-only profile intervention through public SQLite APIs.
 * Production first validates WAL/FULL unchanged. Before the first mutation,
 * this separately injected library selects MEMORY/OFF and records real readback.
 * No SQL result is fabricated and no dependency or production source is patched.
 */
#include <sqlite3.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <time.h>
struct Settings { char journal[16]; long long sync, fk, page, cache, mmap, full, checkpoint; };
struct Record { sqlite3 *db; int live, changed, closed; struct Settings before, effective, final; unsigned long long override_ns; };
static struct Record records[16];
static int used, errors, profile_exec_calls;
static unsigned long long profile_calls,profile_vm,profile_ns;
static pthread_mutex_t mutex=PTHREAD_MUTEX_INITIALIZER;
static _Thread_local int nested;
static unsigned long long now(void){struct timespec t;if(clock_gettime(CLOCK_MONOTONIC_RAW,&t))abort();return (unsigned long long)t.tv_sec*1000000000+(unsigned long long)t.tv_nsec;}
int cause_memory_profile_active(void){return nested;}
static int scalar(sqlite3 *db,const char *sql,char *text,size_t size,long long *number){
 unsigned long long start=now();
 sqlite3_stmt *stmt=0;int rc=sqlite3_prepare_v2(db,sql,-1,&stmt,0);
 if(rc==SQLITE_OK){rc=sqlite3_step(stmt);if(rc==SQLITE_ROW){if(text){const unsigned char *v=sqlite3_column_text(stmt,0);if(!v||strlen((const char*)v)>=size)rc=SQLITE_ERROR;else strcpy(text,(const char*)v);}else *number=sqlite3_column_int64(stmt,0);if(rc==SQLITE_ROW)rc=SQLITE_OK;}else rc=SQLITE_ERROR;}
 if(stmt)profile_vm+=(unsigned long long)sqlite3_stmt_status(stmt,SQLITE_STMTSTATUS_VM_STEP,0);
 int end=sqlite3_finalize(stmt);profile_calls++;profile_ns+=now()-start;return rc==SQLITE_OK?end:rc;
}
static int settings(sqlite3 *db,struct Settings *s){
 if(scalar(db,"PRAGMA journal_mode",s->journal,sizeof(s->journal),0)!=SQLITE_OK)return SQLITE_ERROR;
 const char *sql[]={"PRAGMA synchronous","PRAGMA foreign_keys","PRAGMA page_size","PRAGMA cache_size","PRAGMA mmap_size","PRAGMA fullfsync","PRAGMA checkpoint_fullfsync"};
 long long *out[]={&s->sync,&s->fk,&s->page,&s->cache,&s->mmap,&s->full,&s->checkpoint};
 for(int i=0;i<7;i++)if(scalar(db,sql[i],0,0,out[i])!=SQLITE_OK)return SQLITE_ERROR;
 return SQLITE_OK;
}
static int target(sqlite3 *db){const char *file=sqlite3_db_filename(db,"main"),*prefix=getenv("LAYERFS_CAUSE_MEMORY_DB");return file&&prefix&&!strncmp(file,prefix,strlen(prefix))&&(file[strlen(prefix)]==0||!strcmp(file+strlen(prefix),".history.sqlite"));}
static const char *skip(const char *sql){for(;;){while(isspace((unsigned char)*sql))sql++;if(sql[0]=='-'&&sql[1]=='-'){while(*sql&&*sql!='\n')sql++;}else if(sql[0]=='/'&&sql[1]=='*'){const char *end=strstr(sql+2,"*/");if(!end)return sql;sql=end+2;}else return sql;}}
static int mutation(const char *sql){if(!sql)return 0;sql=skip(sql);const char *words[]={"BEGIN IMMEDIATE","CREATE","INSERT","UPDATE","DELETE","REPLACE","ALTER","DROP"};for(int i=0;i<8;i++){size_t n=strlen(words[i]);if(!sqlite3_strnicmp(sql,words[i],(int)n)&&(!sql[n]||isspace((unsigned char)sql[n])||sql[n]==';'))return 1;}return 0;}
static int before(sqlite3 *db,const char *sql,int bytes){
 char prefix[512];if(!sql)return SQLITE_OK;
 size_t bound=bytes<0?sizeof(prefix)-1:(size_t)bytes;
 if(bound>=sizeof(prefix))bound=sizeof(prefix)-1;
 size_t len=strnlen(sql,bound);memcpy(prefix,sql,len);prefix[len]=0;
 if(nested||!mutation(prefix)||!target(db)||sqlite3_db_readonly(db,"main")!=0)return SQLITE_OK;
 nested=1;pthread_mutex_lock(&mutex);
 for(int i=0;i<used;i++)if(records[i].live&&records[i].db==db){pthread_mutex_unlock(&mutex);nested=0;return SQLITE_OK;}
 int rc=SQLITE_OK;const char *arm=getenv("LAYERFS_CAUSE_MEMORY_ARM");
 if(used==16||!arm||(!strcmp(arm,"candidate")?0:strcmp(arm,"baseline"))||!sqlite3_get_autocommit(db)||sqlite3_db_readonly(db,"main")!=0)rc=SQLITE_ERROR;
 struct Record *r=rc==SQLITE_OK?&records[used++]:0;
 if(r){r->db=db;r->live=1;rc=settings(db,&r->before);unsigned long long start=now();
  if(rc==SQLITE_OK&&!strcmp(arm,"candidate")){
   if(strcmp(r->before.journal,"wal")||r->before.sync!=2)rc=SQLITE_ERROR;
   else {char mode[16];rc=scalar(db,"PRAGMA journal_mode=MEMORY",mode,sizeof(mode),0);if(rc==SQLITE_OK){profile_exec_calls++;rc=sqlite3_exec(db,"PRAGMA synchronous=OFF",0,0,0);}r->changed=1;}
  }
  if(rc==SQLITE_OK)rc=settings(db,&r->effective);r->override_ns=now()-start;
  if(rc==SQLITE_OK&&(strcmp(r->effective.journal,"memory")||r->effective.sync!=0||r->effective.fk!=1||r->effective.page!=4096||r->effective.cache!=r->before.cache||r->effective.mmap!=r->before.mmap||r->effective.full!=r->before.full||r->effective.checkpoint!=r->before.checkpoint))rc=SQLITE_ERROR;
 }
 if(rc!=SQLITE_OK)errors++;
 pthread_mutex_unlock(&mutex);nested=0;return rc;
}
static int prepare2(sqlite3 *db,const char *sql,int n,sqlite3_stmt **stmt,const char **tail){int rc=before(db,sql,n);if(rc!=SQLITE_OK){if(stmt)*stmt=0;return rc;}return sqlite3_prepare_v2(db,sql,n,stmt,tail);}
static int prepare3(sqlite3 *db,const char *sql,int n,unsigned flags,sqlite3_stmt **stmt,const char **tail){int rc=before(db,sql,n);if(rc!=SQLITE_OK){if(stmt)*stmt=0;return rc;}return sqlite3_prepare_v3(db,sql,n,flags,stmt,tail);}
static void closing(sqlite3 *db){if(nested)return;nested=1;pthread_mutex_lock(&mutex);for(int i=0;i<used;i++)if(records[i].live&&records[i].db==db){struct Record*r=&records[i];if(settings(db,&r->final)!=SQLITE_OK||strcmp(r->final.journal,"memory")||r->final.sync!=0)errors++;r->live=0;r->closed=1;}pthread_mutex_unlock(&mutex);nested=0;}
static int close1(sqlite3 *db){closing(db);return sqlite3_close(db);}
static int close2(sqlite3 *db){closing(db);return sqlite3_close_v2(db);}
struct Hook{const void *replacement,*original;};
__attribute__((used,section("__DATA,__interpose,interposing")))static const struct Hook hooks[]={{(const void*)&prepare2,(const void*)&sqlite3_prepare_v2},{(const void*)&prepare3,(const void*)&sqlite3_prepare_v3},{(const void*)&close1,(const void*)&sqlite3_close},{(const void*)&close2,(const void*)&sqlite3_close_v2}};
static void output(FILE*f,const struct Settings*s){fprintf(f,"{\"journal_mode\":\"%s\",\"synchronous\":%lld,\"foreign_keys\":%lld,\"page_size\":%lld,\"cache_size\":%lld,\"mmap_size\":%lld,\"fullfsync\":%lld,\"checkpoint_fullfsync\":%lld}",s->journal,s->sync,s->fk,s->page,s->cache,s->mmap,s->full,s->checkpoint);}
__attribute__((destructor))static void report(void){const char *path=getenv("LAYERFS_CAUSE_MEMORY_LOG");if(!path)return;FILE*f=fopen(path,"wx");if(!f){fprintf(stderr,"memory profile report refused\n");return;}fprintf(f,"{\"kind\":\"diagnostic-before-first-mutation-memory-off\",\"sqlite_version\":\"%s\",\"errors\":%d,\"observer_scalar_calls\":%llu,\"observer_scalar_vm\":%llu,\"observer_scalar_ns\":%llu,\"observer_exec_calls\":%d,\"connections\":[",sqlite3_libversion(),errors,profile_calls,profile_vm,profile_ns,profile_exec_calls);for(int i=0;i<used;i++){struct Record*r=&records[i];if(i)fputc(',',f);fprintf(f,"{\"changed\":%d,\"closed\":%d,\"live\":%d,\"override_ns\":%llu,\"before\":",r->changed,r->closed,r->live,r->override_ns);output(f,&r->before);fprintf(f,",\"effective\":");output(f,&r->effective);fprintf(f,",\"final\":");output(f,&r->final);fputc('}',f);}fprintf(f,"]}\n");if(fclose(f))fprintf(stderr,"memory profile report close failed\n");}
