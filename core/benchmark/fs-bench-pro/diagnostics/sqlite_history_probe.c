#include <sqlite3.h>
#include <stdint.h>
#include <dlfcn.h>
#include <assert.h>
#include <stdio.h>
int main(void){
 sqlite3 *db=0;sqlite3_stmt *stmt=0;sqlite3_blob *blob=0;uint64_t before[20],after[20];char byte;
 void(*snapshot)(uint64_t*)=dlsym(RTLD_DEFAULT,"cause_history_snapshot");assert(snapshot);
 assert(sqlite3_open_v2(":memory:",&db,SQLITE_OPEN_READWRITE|SQLITE_OPEN_CREATE,0)==SQLITE_OK);
 assert(sqlite3_exec(db,"CREATE TABLE pack(pack_id INTEGER PRIMARY KEY,body BLOB); INSERT INTO pack VALUES(1,zeroblob(8))",0,0,0)==SQLITE_OK);
 snapshot(before);
 assert(sqlite3_prepare_v2(db,"SELECT body FROM pack WHERE pack_id=1",-1,&stmt,0)==SQLITE_OK);
 assert(sqlite3_step(stmt)==SQLITE_ROW);assert(sqlite3_step(stmt)==SQLITE_DONE);assert(sqlite3_finalize(stmt)==SQLITE_OK);
 assert(sqlite3_blob_open(db,"main","pack","body",1,0,&blob)==SQLITE_OK);assert(sqlite3_blob_read(blob,&byte,1,0)==SQLITE_OK);assert(sqlite3_blob_close(blob)==SQLITE_OK);
 assert(sqlite3_blob_open(db,"main","pack","body",1,1,&blob)==SQLITE_OK);assert(sqlite3_blob_close(blob)==SQLITE_OK);
 assert(sqlite3_blob_open(db,"main","pack","body",99,0,&blob)!=SQLITE_OK);
 uint64_t work[12];void(*blob_snapshot)(uint64_t*)=dlsym(RTLD_DEFAULT,"cause_history_blob_snapshot");assert(blob_snapshot);
 assert(sqlite3_blob_open(db,"main","pack","body",1,0,&blob)==SQLITE_OK);
 assert(sqlite3_blob_read(blob,&byte,1,8)!=SQLITE_OK);assert(sqlite3_blob_close(blob)==SQLITE_OK);
 blob_snapshot(work);assert(work[11]==1);assert(work[0]==3);assert(work[2]==1);assert(work[3]==2);assert(work[4]==2);assert(work[5]==1);assert(work[7]==1);assert(work[8]>=3);
 snapshot(after);assert(after[15]-before[15]==3);assert(after[16]-before[16]==1);assert(after[17]-before[17]==0);
 assert(sqlite3_close(db)==SQLITE_OK);puts("PASS pack attribution plus BLOB attempts/returned bytes/errors; operations delegated unchanged");
}
