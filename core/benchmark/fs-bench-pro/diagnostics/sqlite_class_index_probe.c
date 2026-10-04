/* Real delegated SQLite counter-key calibration, no product/benchmark sample. */
#include <sqlite3.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <assert.h>
static uint64_t hash(const char*s){uint64_t h=UINT64_C(14695981039346656037);for(;*s;s++)h=(h^(unsigned char)*s)*UINT64_C(1099511628211);return h;}
int main(int argc,char**argv){
 assert(argc==2);int values[12],found=0;char sql[96],path[4096];
 for(int n=0;n<1000000&&found<12;n++){snprintf(sql,sizeof(sql),"SELECT %d AS value",n);if(hash(sql)%1024==0)values[found++]=n;}
 assert(found==12);const char*names[]={":memory:","store.sqlite","store.sqlite.history.sqlite"};const char*phases[]={"bootstrap","operation","cleanup"};
 for(int owner=0;owner<3;owner++){
  if(owner)snprintf(path,sizeof(path),"%s/%s",argv[1],names[owner]);else snprintf(path,sizeof(path),"%s",names[owner]);
  sqlite3 *db=0;sqlite3_stmt*statements[12];assert(sqlite3_open_v2(path,&db,SQLITE_OPEN_READWRITE|SQLITE_OPEN_CREATE,0)==SQLITE_OK);
  for(int n=0;n<12;n++){snprintf(sql,sizeof(sql),"SELECT %d AS value",values[n]);assert(sqlite3_prepare_v2(db,sql,-1,&statements[n],0)==SQLITE_OK);}
  for(int phase=0;phase<3;phase++){
   assert(setenv("LAYERFS_SQLITE_SCOPE",phases[phase],1)==0);
   for(int repetition=0;repetition<2;repetition++)for(int n=0;n<12;n++){
    assert(sqlite3_step(statements[n])==SQLITE_ROW);assert(sqlite3_column_int(statements[n],0)==values[n]);assert(sqlite3_step(statements[n])==SQLITE_DONE);assert(sqlite3_reset(statements[n])==SQLITE_OK);
   }
  }
  for(int n=0;n<12;n++)assert(sqlite3_finalize(statements[n])==SQLITE_OK);assert(sqlite3_close(db)==SQLITE_OK);
 }
 puts("PASS bucket collisions, owner/phase keys, reused statements and exact values");return 0;
}
