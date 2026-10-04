/* Labelled close mechanism observer. Delegates unchanged public SQLite/VFS/OS
 * operations; no retry, profile override, dependency patch or product hook. */
#include "sqlite_vfs_observer.c"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <pthread.h>
#include <unistd.h>
static pthread_once_t initialization = PTHREAD_ONCE_INIT;
static int initialize_result;
static _Atomic unsigned long long sql_calls[2],sql_ns[2],fd_calls[3],fd_ns[3],changed;
static void initialize_once(void){initialize_result=cause_vfs_initialize();}
static int observed_open(const char *path,sqlite3 **db,int flag,const char *vfs){
 pthread_once(&initialization,initialize_once);
 if(initialize_result!=SQLITE_OK)return initialize_result;
 return sqlite3_open_v2(path,db,flag,vfs);
}
static int observed_close(sqlite3 *db){
 int mutation=db&&sqlite3_total_changes(db)>0;uint64_t start=now();int rc=sqlite3_close(db);
 atomic_fetch_add(&sql_calls[0],1);atomic_fetch_add(&sql_ns[0],now()-start);if(mutation)atomic_fetch_add(&changed,1);return rc;
}
static int observed_close_v2(sqlite3 *db){
 int mutation=db&&sqlite3_total_changes(db)>0;uint64_t start=now();int rc=sqlite3_close_v2(db);
 atomic_fetch_add(&sql_calls[1],1);atomic_fetch_add(&sql_ns[1],now()-start);if(mutation)atomic_fetch_add(&changed,1);return rc;
}
static int observed_fd_close(int fd){
 int prior=errno;char path[PATH_MAX];int kind=-1;
 if(!fcntl(fd,F_GETPATH,path))kind=strstr(path,".layerfs-allocation-")?2:strstr(path,"history.sqlite")?1:strstr(path,"store.sqlite")?0:-1;
 errno=prior;uint64_t start=now();int rc=close(fd);int after=errno;
 if(kind>=0){atomic_fetch_add(&fd_calls[kind],1);atomic_fetch_add(&fd_ns[kind],now()-start);}errno=after;return rc;
}
struct Binding{const void *replacement;const void *original;};
__attribute__((used,section("__DATA,__interpose,interposing")))
static const struct Binding bindings[]={
 {(const void*)&observed_open,(const void*)&sqlite3_open_v2},
 {(const void*)&observed_close,(const void*)&sqlite3_close},
 {(const void*)&observed_close_v2,(const void*)&sqlite3_close_v2},
 {(const void*)&observed_fd_close,(const void*)&close}};
__attribute__((destructor))static void close_report(void){
 if(!atomic_load(&changed))return;
 const char *path=getenv("LAYERFS_CLOSE_OBSERVER_OUTPUT");if(!path)return;
 FILE *out=fopen(path,"wx");if(!out){fprintf(stderr,"close observer output refused\n");return;}
 fprintf(out,"{\"kind\":\"close-mechanism-diagnostic-v1\",\"mutating_connection_closes\":%llu,\"sqlite_close_calls\":%llu,\"sqlite_close_ns\":%llu,\"sqlite_close_v2_calls\":%llu,\"sqlite_close_v2_ns\":%llu,\"fd_closes\":[",atomic_load(&changed),atomic_load(&sql_calls[0]),atomic_load(&sql_ns[0]),atomic_load(&sql_calls[1]),atomic_load(&sql_ns[1]));
 for(int i=0;i<3;i++){if(i)fputc(',',out);fprintf(out,"{\"class\":\"%s\",\"calls\":%llu,\"wall_ns\":%llu}",i==0?"main":i==1?"history":"allocation-scratch",atomic_load(&fd_calls[i]),atomic_load(&fd_ns[i]));}
 fprintf(out,"]}\n");fclose(out);
}
