/* Bounded observer capability proof over public SQLite APIs, not a workload. */
#include <sqlite3.h>
#include <dlfcn.h>
#include <stdio.h>
typedef int (*Initialize)(void);
static int run(sqlite3 *db,const char *sql){
 sqlite3_stmt*s=0;int rc=sqlite3_prepare_v2(db,sql,-1,&s,0);
 if(rc!=SQLITE_OK)return rc;
 do{rc=sqlite3_step(s);}while(rc==SQLITE_ROW);
 int close=sqlite3_finalize(s);return rc==SQLITE_DONE?close:rc;
}
int main(int argc,char**argv){
 if(argc!=2)return 2;
 Initialize initialize=(Initialize)dlsym(RTLD_DEFAULT,"cause_vfs_initialize");
 if(!initialize||initialize()!=SQLITE_OK)return 3;
 sqlite3*db=0;
 int rc=sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READWRITE|SQLITE_OPEN_CREATE,0);
 if(rc!=SQLITE_OK)return 4;
 const char*sql[]={"PRAGMA journal_mode=WAL","PRAGMA synchronous=FULL","PRAGMA fullfsync=ON","PRAGMA checkpoint_fullfsync=ON","PRAGMA foreign_keys=ON","PRAGMA page_size=4096","PRAGMA wal_autocheckpoint=1000","BEGIN IMMEDIATE","CREATE TABLE proof(x INTEGER)","INSERT INTO proof VALUES(42)","COMMIT","PRAGMA wal_checkpoint(TRUNCATE)"};
 for(size_t i=0;i<sizeof(sql)/sizeof(*sql);i++){rc=run(db,sql[i]);if(rc!=SQLITE_OK){fprintf(stderr,"probe refused %zu rc=%d\n",i,rc);return 5;}}
 sqlite3_stmt*s=0;rc=sqlite3_prepare_v2(db,"SELECT x FROM proof",-1,&s,0);
 if(rc!=SQLITE_OK||sqlite3_step(s)!=SQLITE_ROW||sqlite3_column_int(s,0)!=42)return 6;
 if(sqlite3_step(s)!=SQLITE_DONE||sqlite3_finalize(s)!=SQLITE_OK||sqlite3_close(db)!=SQLITE_OK)return 7;
 printf("{\"observer_capability\":\"PASS\",\"readback\":42,\"sqlite_version\":\"%s\"}\n",sqlite3_libversion());return 0;
}
