/* Capability diagnostic only; no product fixture, sample, or VFS claim. */
#include <sqlite3.h>
#include <stdint.h>
#include <stdio.h>
static unsigned long long calls,vm,ns,connections,errors;
static int profile(unsigned event,void *context,void *stmt,void *duration){
 (void)event;(void)context;calls++;
 vm+=(unsigned)sqlite3_stmt_status((sqlite3_stmt*)stmt,SQLITE_STMTSTATUS_VM_STEP,1);
 ns+=*(sqlite3_uint64*)duration;return 0;
}
static void opened(void *context,sqlite3 *db,const char *sql,int event){
 (void)context;(void)sql;
 if(event==0){connections++;if(sqlite3_trace_v2(db,SQLITE_TRACE_PROFILE,profile,NULL)!=SQLITE_OK)errors++;}
}
int main(void){
 int configured=sqlite3_config(SQLITE_CONFIG_SQLLOG,opened,NULL);
 sqlite3 *db=NULL;int result=sqlite3_open(":memory:",&db);
 if(result==SQLITE_OK)result=sqlite3_exec(db,"SELECT 1",NULL,NULL,NULL);
 if(db)sqlite3_close(db);
 printf("{\"kind\":\"sqlite-observer-capability\",\"version\":\"%s\",\"configured\":%d,\"result\":%d,\"connections\":%llu,\"profile_calls\":%llu,\"vm_steps\":%llu,\"engine_profile_ns\":%llu,\"errors\":%llu}\n",sqlite3_libversion(),configured,result,connections,calls,vm,ns,errors);
 return configured!=SQLITE_OK||result!=SQLITE_OK||errors||!calls;
}
