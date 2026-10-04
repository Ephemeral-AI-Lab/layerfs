#include <sqlite3.h>
#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
int main(int argc,char **argv){
 if(argc!=3)return 2;
 sqlite3 *db=0;int r=sqlite3_open(argv[1],&db);if(r)return 3;
 if(sqlite3_exec(db,"PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA fullfsync=ON;CREATE TABLE t(a);INSERT INTO t VALUES(1);PRAGMA wal_checkpoint(TRUNCATE);",0,0,0))return 4;
 int persist=-1; r=sqlite3_file_control(db,"main",SQLITE_FCNTL_PERSIST_WAL,&persist);
 int extra=atoi(argv[2])?open(argv[1],O_RDWR):-1;
 int close_result=sqlite3_close(db);if(extra>=0)close(extra);
 char path[4096];struct stat st;
 snprintf(path,sizeof(path),"%s-wal",argv[1]);int wal=stat(path,&st)==0;long long wal_size=wal?st.st_size:-1;
 snprintf(path,sizeof(path),"%s-shm",argv[1]);int shm=stat(path,&st)==0;long long shm_size=shm?st.st_size:-1;
 printf("{\"persist_wal_result\":%d,\"persist_wal\":%d,\"close_result\":%d,\"wal_exists\":%d,\"wal_size\":%lld,\"shm_exists\":%d,\"shm_size\":%lld}\n",r,persist,close_result,wal,wal_size,shm,shm_size);
 return close_result?5:0;
}
