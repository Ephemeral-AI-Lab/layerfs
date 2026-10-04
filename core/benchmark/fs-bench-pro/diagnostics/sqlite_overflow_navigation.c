/* Count-only public SQLite API vehicle. No LayerFS performance/admission claim. */
#include <sqlite3.h>
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
int main(int argc,char **argv){
 assert(argc==3);int offset=atoi(argv[2]);assert(offset>=0);
 uint64_t before[20],after[20];sqlite3 *db=0;sqlite3_blob *blob=0;
 void(*snapshot)(uint64_t*)=dlsym(RTLD_DEFAULT,"cause_history_snapshot");assert(snapshot);
 snapshot(before);
 assert(sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READONLY,0)==SQLITE_OK);
 assert(sqlite3_exec(db,"PRAGMA cache_size=-2048;PRAGMA mmap_size=0;PRAGMA temp_store=2;BEGIN",0,0,0)==SQLITE_OK);
 assert(sqlite3_blob_open(db,"main","pack","body",1,0,&blob)==SQLITE_OK);
 unsigned char bytes[32768];
 assert(sqlite3_blob_read(blob,bytes,sizeof(bytes),offset)==SQLITE_OK);
 for(size_t i=0;i<sizeof(bytes);i++)assert(bytes[i]==0);
 assert(sqlite3_blob_close(blob)==SQLITE_OK);
 assert(sqlite3_exec(db,"COMMIT",0,0,0)==SQLITE_OK);
 assert(sqlite3_close(db)==SQLITE_OK);snapshot(after);
 printf("{\"kind\":\"count-only-overflow-navigation\",\"sqlite\":\"%s\",\"offset\":%d,\"returned_bytes\":32768,\"vfs_requested_bytes\":%llu,\"acquisitions\":%llu}\n",sqlite3_libversion(),offset,(unsigned long long)(after[11]-before[11]),(unsigned long long)(after[15]-before[15]));
 return 0;
}
