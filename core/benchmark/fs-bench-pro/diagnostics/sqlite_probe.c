#include <sqlite3.h>
#include <stdlib.h>
#include <stdio.h>
int main(void) {
 sqlite3 *db=NULL;
 if(sqlite3_open_v2(":memory:",&db,SQLITE_OPEN_READWRITE|SQLITE_OPEN_CREATE,NULL)!=SQLITE_OK)return 1;
 if(sqlite3_exec(db,"CREATE TABLE probe(x);BEGIN;INSERT INTO probe VALUES(1),(2);COMMIT;",NULL,NULL,NULL)!=SQLITE_OK)return 2;
 setenv("LAYERFS_SQLITE_SCOPE","operation",1);
 sqlite3_stmt *s=NULL;
 if(sqlite3_prepare_v2(db,"SELECT x FROM probe ORDER BY x",-1,&s,NULL)!=SQLITE_OK)return 3;
 int rows=0;while(sqlite3_step(s)==SQLITE_ROW)rows++;
 sqlite3_finalize(s);sqlite3_close(db);return rows==2?0:4;
}
