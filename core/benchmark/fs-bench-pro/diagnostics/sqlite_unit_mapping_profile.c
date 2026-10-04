/* Count/cause diagnostic on a closed owned Store; never a gate sample. */
#include <sqlite3.h>
#include <stdio.h>
#include <stdint.h>
#include <time.h>
static uint64_t nanos(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
int main(int argc,char **argv){
 if(argc!=2)return 2;
 sqlite3 *db=0;sqlite3_stmt *ids=0,*q=0;int rc,now,high;
 if(sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READONLY|SQLITE_OPEN_NOMUTEX,0)!=SQLITE_OK)return 3;
 if(sqlite3_exec(db,"PRAGMA cache_size=-2048;PRAGMA mmap_size=0;PRAGMA temp_store=2;BEGIN",0,0,0)!=SQLITE_OK)return 4;
 if(sqlite3_prepare_v2(db,"SELECT pack_id FROM pack ORDER BY pack_id",-1,&ids,0)!=SQLITE_OK)return 5;
 sqlite3_int64 packs[4096];int count=0;
 while((rc=sqlite3_step(ids))==SQLITE_ROW){if(count==4096)return 6;packs[count++]=sqlite3_column_int64(ids,0);}
 if(rc!=SQLITE_DONE||sqlite3_finalize(ids)!=SQLITE_OK)return 7;
 if(sqlite3_prepare_v2(db,"SELECT unit_id,group_number,offset,length FROM pack_unit WHERE pack_id=?1 ORDER BY group_number LIMIT 257",-1,&q,0)!=SQLITE_OK)return 8;
 if(sqlite3_db_release_memory(db)!=SQLITE_OK)return 9;
 sqlite3_db_status(db,SQLITE_DBSTATUS_CACHE_HIT,&now,&high,1);sqlite3_db_status(db,SQLITE_DBSTATUS_CACHE_MISS,&now,&high,1);
 uint64_t start=nanos(),rows=0,steps=0,checksum=0;
 for(int i=0;i<count;i++){
  if(sqlite3_bind_int64(q,1,packs[i])!=SQLITE_OK)return 10;
  while((rc=sqlite3_step(q))==SQLITE_ROW){rows++;for(int k=0;k<4;k++)checksum+=(uint64_t)sqlite3_column_int64(q,k);}
  if(rc!=SQLITE_DONE)return 11;
  steps+=(uint64_t)sqlite3_stmt_status(q,SQLITE_STMTSTATUS_VM_STEP,1);
  if(sqlite3_reset(q)!=SQLITE_OK)return 12;
 }
 uint64_t wall=nanos()-start;int hits,misses;
 sqlite3_db_status(db,SQLITE_DBSTATUS_CACHE_HIT,&hits,&high,0);sqlite3_db_status(db,SQLITE_DBSTATUS_CACHE_MISS,&misses,&high,0);
 printf("{\"kind\":\"count-cause-once-per-pack\",\"sqlite\":\"%s\",\"queries\":%d,\"rows\":%llu,\"vm_steps\":%llu,\"checksum\":%llu,\"cache_hits\":%d,\"cache_misses\":%d,\"wall_ns\":%llu,\"cache_state\":\"SQLite cache released; OS cache uncontrolled; diagnostic only\"}\n",sqlite3_libversion(),count,(unsigned long long)rows,(unsigned long long)steps,(unsigned long long)checksum,hits,misses,(unsigned long long)wall);
 if(sqlite3_finalize(q)!=SQLITE_OK||sqlite3_exec(db,"COMMIT",0,0,0)!=SQLITE_OK||sqlite3_close(db)!=SQLITE_OK)return 13;
 return 0;
}
