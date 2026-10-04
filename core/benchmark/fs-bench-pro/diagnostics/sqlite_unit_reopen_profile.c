/* Count/cause once per multi-group pack; uncontrolled OS cache, no gate sample. */
#include <sqlite3.h>
#include <stdio.h>
#include <stdint.h>
#include <time.h>
#include <stdlib.h>
static uint64_t now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
int main(int argc,char **argv){
 if(argc!=3)return 2;int reuse=atoi(argv[2]);sqlite3 *db=0;sqlite3_stmt *q=0;sqlite3_blob *blob=0;
 if(sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READONLY|SQLITE_OPEN_NOMUTEX,0)!=SQLITE_OK)return 3;
 if(sqlite3_exec(db,"PRAGMA cache_size=-2048;PRAGMA mmap_size=0;BEGIN",0,0,0)!=SQLITE_OK)return 4;
 if(sqlite3_prepare_v2(db,"SELECT pack_id,unit_id,length FROM pack_unit WHERE pack_id IN(SELECT pack_id FROM pack_unit GROUP BY pack_id HAVING count(*)>1) ORDER BY pack_id,group_number",-1,&q,0)!=SQLITE_OK)return 5;
 struct row{sqlite3_int64 pack,id;int length;}rows[16384];int count=0,rc;
 while((rc=sqlite3_step(q))==SQLITE_ROW){if(count==16384)return 6;rows[count++]=(struct row){sqlite3_column_int64(q,0),sqlite3_column_int64(q,1),sqlite3_column_int(q,2)};if(rows[count-1].length<1||rows[count-1].length>2097152)return 7;}
 if(rc!=SQLITE_DONE||sqlite3_finalize(q)!=SQLITE_OK||sqlite3_db_release_memory(db)!=SQLITE_OK)return 8;
 unsigned char *buffer=malloc(2097152);if(!buffer)return 9;uint64_t start=now(),opens=0,reopens=0,closes=0,bytes=0,sum=0;sqlite3_int64 pack=0;
 for(int i=0;i<count;i++){
  if(!reuse||rows[i].pack!=pack){if(blob){if(sqlite3_blob_close(blob)!=SQLITE_OK)return 10;closes++;blob=0;}
   if(sqlite3_blob_open(db,"main","pack_unit","body",rows[i].id,0,&blob)!=SQLITE_OK)return 11;opens++;
  }else{if(sqlite3_blob_reopen(blob,rows[i].id)!=SQLITE_OK)return 12;reopens++;}
  if(sqlite3_blob_bytes(blob)!=rows[i].length||sqlite3_blob_read(blob,buffer,rows[i].length,0)!=SQLITE_OK)return 13;
  bytes+=(uint64_t)rows[i].length;for(int n=0;n<rows[i].length;n++)sum+=buffer[n];pack=rows[i].pack;
 }
 if(blob){if(sqlite3_blob_close(blob)!=SQLITE_OK)return 14;closes++;}
 uint64_t wall=now()-start;free(buffer);
 printf("{\"kind\":\"count-cause-once-per-multigroup-pack\",\"sqlite\":\"%s\",\"mode\":\"%s\",\"rows\":%d,\"opens\":%llu,\"reopens\":%llu,\"closes\":%llu,\"bytes\":%llu,\"byte_sum\":%llu,\"wall_ns\":%llu,\"cache_state\":\"SQLite cache released; OS cache uncontrolled; diagnostic only\"}\n",sqlite3_libversion(),reuse?"reopen":"open-close",count,(unsigned long long)opens,(unsigned long long)reopens,(unsigned long long)closes,(unsigned long long)bytes,(unsigned long long)sum,(unsigned long long)wall);
 if(sqlite3_exec(db,"COMMIT",0,0,0)!=SQLITE_OK||sqlite3_close(db)!=SQLITE_OK)return 15;return 0;
}
