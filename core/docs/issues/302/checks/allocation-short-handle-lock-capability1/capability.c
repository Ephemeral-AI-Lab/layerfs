#include <sqlite3.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/wait.h>
#include <stdio.h>
#include <string.h>
int main(int argc,char**argv){
 if(argc<2)return 1;sqlite3 *db=0;
 if(sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_CREATE|SQLITE_OPEN_READWRITE,0))return 2;
 if(argc==3){int rc=sqlite3_exec(db,"BEGIN IMMEDIATE",0,0,0);printf("writer_rc=%d\n",rc);sqlite3_close(db);return rc==SQLITE_BUSY?0:3;}
 if(sqlite3_exec(db,"PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;CREATE TABLE t(x);BEGIN IMMEDIATE;INSERT INTO t VALUES(1)",0,0,0))return 4;
 int fd=open(argv[1],O_RDWR);if(fd<0)return 5;if(close(fd))return 6;
 pid_t pid=fork();if(pid<0)return 7;if(!pid){execl(argv[0],argv[0],argv[1],"try",(char*)0);_exit(8);}
 int status;if(waitpid(pid,&status,0)<0)return 9;
 sqlite3_exec(db,"ROLLBACK",0,0,0);sqlite3_close(db);
 printf("sqlite_version=%s extra_fd_closed_writer_blocked=%d\n",sqlite3_libversion(),WIFEXITED(status)&&WEXITSTATUS(status)==0);
 return WIFEXITED(status)?WEXITSTATUS(status):10;
}
