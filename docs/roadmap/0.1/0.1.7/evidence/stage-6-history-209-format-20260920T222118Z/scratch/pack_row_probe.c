/* D3 -- which pack-row write path actually stops the whole-chain rewrite.
 *
 * The append hands SQLite the whole assembled pack today, and the pack layout puts
 * the group directory at the FRONT, so adding a 16-byte directory entry shifts every
 * body byte: even a same-size payload changes every page. This probe prices the
 * three candidate write paths on a copy of the measured run's own Store, each arm
 * interleaved round-robin in one process, reading SQLITE_DBSTATUS_CACHE_WRITE around
 * every COMMIT so a step is never priced without the pages it wrote.
 *
 *   cc -O2 -o pack_row_probe pack_row_probe.c -lsqlite3
 *   ./pack_row_probe <store-copy.sqlite> <rounds>
 */
#include <sqlite3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define ROWID 999999
#define HEADER 16
#define DIRENT 16
#define DIR_RESERVE (256 * DIRENT)   /* group_count_limit directory entries */
#define BODY_START (HEADER + DIR_RESERVE)
#define PACK_LIMIT (256 * 1024)
#define ROW_LEN (BODY_START + PACK_LIMIT)
#define GROUP_BODY 1450
#define GROUPS 170                   /* ~246 KiB of bodies: near the pack limit */

static double now_us(void){ struct timespec ts; clock_gettime(CLOCK_MONOTONIC,&ts);
  return (double)ts.tv_sec*1e6 + (double)ts.tv_nsec/1e3; }

static void check(int rc, sqlite3 *db, const char *what){
  if(rc!=SQLITE_OK && rc!=SQLITE_ROW && rc!=SQLITE_DONE){
    fprintf(stderr,"%s: %s (%d)\n",what,sqlite3_errmsg(db),rc); exit(1); } }

static void exec(sqlite3 *db, const char *sql){ char *e=NULL;
  if(sqlite3_exec(db,sql,NULL,NULL,&e)!=SQLITE_OK){ fprintf(stderr,"%s: %s\n",sql,e?e:"?"); exit(1);} }

static long long cache_write(sqlite3 *db){ int c=0,h=0;
  sqlite3_db_status(db,SQLITE_DBSTATUS_CACHE_WRITE,&c,&h,0); return c; }

/* One arm: `rounds` steps, each BEGIN; write; COMMIT; pages counted at the COMMIT. */
static void run_arm(sqlite3 *db, int arm, int rounds, unsigned char *row,
                    sqlite3_stmt *upd, long long *pages, long long *commits,
                    long long *writes_ns, long long *tail_bytes){
  sqlite3_blob *blob = NULL;
  int groups = 1;
  if(arm==3){
    /* The pre-allocated row the incremental path writes into. */
    exec(db,"DELETE FROM object_packs WHERE pack_id=999999");
    sqlite3_stmt *ins=NULL;
    check(sqlite3_prepare_v2(db,"INSERT INTO object_packs (pack_id,data,save_id) VALUES (999999, zeroblob(?1), (SELECT save_id FROM object_packs ORDER BY pack_id LIMIT 1))",-1,&ins,NULL),db,"prep ins");
    sqlite3_bind_int64(ins,1,ROW_LEN); check(sqlite3_step(ins),db,"ins"); sqlite3_finalize(ins);
  }
  for(int i=0;i<rounds;i++){
    int g = 1 + (i % GROUPS);
    int tail_off = BODY_START + (g-1)*GROUP_BODY;
    exec(db,"BEGIN IMMEDIATE");
    long long before = cache_write(db);
    if(arm==0){
      /* Today: the whole assembled pack, front directory, growing length. */
      int len = BODY_START + g*GROUP_BODY;
      memset(row, (unsigned char)(i & 0xff), (size_t)len);
      exec(db,"DELETE FROM object_packs WHERE pack_id=999999");
      sqlite3_stmt *ins=NULL;
      check(sqlite3_prepare_v2(db,"INSERT INTO object_packs (pack_id,data,save_id) VALUES (999999, ?1, (SELECT save_id FROM object_packs ORDER BY pack_id LIMIT 1))",-1,&ins,NULL),db,"prep");
      sqlite3_bind_blob(ins,1,row,len,SQLITE_TRANSIENT); check(sqlite3_step(ins),db,"ins"); sqlite3_finalize(ins);
    } else if(arm==1){
      /* Same-size row, but the front directory shifts the bodies: today's layout,
         with the directory at HEADER and the bodies after the g entries. The shift
         moves REAL body bytes, which is what an append does to this layout. */
      memmove(row+HEADER+DIRENT, row+HEADER,
              (size_t)((g-1)*DIRENT + (g-1)*GROUP_BODY));
      memset(row+HEADER+(g-1)*DIRENT, (unsigned char)i, DIRENT);
      memset(row+HEADER+g*DIRENT+(g-1)*GROUP_BODY, (unsigned char)(0x40+g), GROUP_BODY);
      sqlite3_bind_int64(upd,1,ROWID);
      sqlite3_bind_blob(upd,2,row,ROW_LEN,SQLITE_TRANSIENT);
      check(sqlite3_step(upd),db,"upd"); sqlite3_reset(upd);
    } else if(arm==2){
      /* Same-size row, directory reserved at a fixed offset, bodies append-only. */
      memset(row+HEADER+(g-1)*DIRENT, (unsigned char)i, DIRENT);
      memset(row+tail_off, (unsigned char)(0x40+g), GROUP_BODY);
      sqlite3_bind_int64(upd,1,ROWID);
      sqlite3_bind_blob(upd,2,row,ROW_LEN,SQLITE_TRANSIENT);
      check(sqlite3_step(upd),db,"upd"); sqlite3_reset(upd);
    } else {
      /* Incremental blob: only the changed byte ranges are written. */
      memset(row+HEADER+(g-1)*DIRENT, (unsigned char)i, DIRENT);
      memset(row+tail_off, (unsigned char)(0x40+g), GROUP_BODY);
      /* flags: 0 opens the handle READ-ONLY and every write then returns
         SQLITE_READONLY; 1 opens it for writing. */
      check(sqlite3_blob_open(db,"main","object_packs","data",ROWID,1,&blob),db,"blob_open");
      check(sqlite3_blob_write(blob,row+HEADER+(g-1)*DIRENT,DIRENT,HEADER+(g-1)*DIRENT),db,"blob_write dir");
      check(sqlite3_blob_write(blob,row+tail_off,GROUP_BODY,tail_off),db,"blob_write body");
      check(sqlite3_blob_close(blob),db,"blob_close"); blob=NULL;
      *tail_bytes += GROUP_BODY;
    }
    exec(db,"COMMIT");
    *pages += cache_write(db)-before;
    *commits += 1;
    groups = g;
  }
  (void)writes_ns; (void)groups;
}

static const char *ARMS[] = {
  "today: whole-row UPDATE, front directory, growing row",
  "same-size row, front directory (bodies shift)",
  "same-size row, reserved directory, append-only tail",
  "incremental blob: directory + tail ranges only",
};
#define NARMS 4

int main(int argc, char **argv){
  setvbuf(stdout,NULL,_IONBF,0);
  if(argc<3){ fprintf(stderr,"usage: %s <store.sqlite> <rounds>\n",argv[0]); return 2; }
  int rounds = atoi(argv[2]);
  sqlite3 *db=NULL; check(sqlite3_open(argv[1],&db),db,"open");
  exec(db,"PRAGMA journal_mode = MEMORY");
  exec(db,"PRAGMA synchronous = OFF");
  exec(db,"PRAGMA temp_store = MEMORY");
  exec(db,"PRAGMA foreign_keys = ON");
  exec(db,"PRAGMA busy_timeout = 0");

  unsigned char *row = calloc(1,(size_t)ROW_LEN);
  memcpy(row,"LFPACK\0\0",8); row[8]=1;
  sqlite3_stmt *upd=NULL;
  check(sqlite3_prepare_v2(db,"UPDATE object_packs SET data = ?2 WHERE pack_id = ?1",-1,&upd,NULL),db,"prep upd");

  long long pages[NARMS]={0}, commits[NARMS]={0}, tns[NARMS]={0}, tails[NARMS]={0};
  double us[NARMS]={0};
  for(int r=0;r<3;r++){
    for(int a=0;a<NARMS;a++){
      double t0=now_us();
      run_arm(db,a,rounds,row,upd,&pages[a],&commits[a],&tns[a],&tails[a]);
      us[a]+= (now_us()-t0)/(double)rounds;
    }
  }
  printf("store=%s rounds=%d row_len=%d sqlite=%s\n",argv[1],rounds,ROW_LEN,sqlite3_libversion());
  printf("%-58s %10s %14s\n","arm","us/step","pages/commit");
  for(int a=0;a<NARMS;a++)
    printf("%-58s %10.1f %14.3f\n",ARMS[a],us[a]/3.0, commits[a]?(double)pages[a]/(double)commits[a]:0.0);
  sqlite3_finalize(upd); sqlite3_close(db); free(row);
  return 0;
}
