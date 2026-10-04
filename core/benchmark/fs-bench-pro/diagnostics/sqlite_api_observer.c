/* First-party diagnostic interposition of public APIs. Same arguments/return,
 * no retry/state/VFS/profile change. Protocol reference: Apple's dyld
 * include/mach-o/dyld-interposing.h (dyld-1042.1); no dependency code is vendored.
 */
#include <sqlite3.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static _Atomic unsigned long long calls[4],wall[4];
static uint64_t now(void){struct timespec value;if(clock_gettime(CLOCK_MONOTONIC_RAW,&value))abort();return (uint64_t)value.tv_sec*1000000000+(uint64_t)value.tv_nsec;}
static int is_commit(sqlite3_stmt *stmt){if(!stmt)return 0;const char *sql=sqlite3_sql(stmt);return sql&&!strcmp(sql,"COMMIT");}
static int observed_step(sqlite3_stmt *stmt){int commit=is_commit(stmt);uint64_t start=now();int rc=sqlite3_step(stmt);uint64_t elapsed=now()-start;
 atomic_fetch_add(&calls[0],1);atomic_fetch_add(&wall[0],elapsed);if(commit){atomic_fetch_add(&calls[2],1);atomic_fetch_add(&wall[2],elapsed);}return rc;}
static int observed_reset(sqlite3_stmt *stmt){int commit=is_commit(stmt);uint64_t start=now();int rc=sqlite3_reset(stmt);uint64_t elapsed=now()-start;
 atomic_fetch_add(&calls[1],1);atomic_fetch_add(&wall[1],elapsed);if(commit){atomic_fetch_add(&calls[3],1);atomic_fetch_add(&wall[3],elapsed);}return rc;}
/* dyld consumes replacement/original address pairs. Calls within this observer
 * retain the imported original symbol, following the documented interpose ABI. */
struct Hook{const void *replacement;const void *original;};
__attribute__((used,section("__DATA,__interpose,interposing")))
static const struct Hook hooks[2]={{(const void*)&observed_step,(const void*)&sqlite3_step},{(const void*)&observed_reset,(const void*)&sqlite3_reset}};
__attribute__((destructor))static void report(void){
 const char *path=getenv("LAYERFS_CAUSE_API_LOG");if(!path)return;
 FILE *file=fopen(path,"wx");if(!file){fprintf(stderr,"cause API observer refused output\n");return;}
 fprintf(file,"{\"kind\":\"sqlite-public-api-delegation-observer\",\"sqlite_version\":\"%s\",\"clock\":\"CLOCK_MONOTONIC_RAW\",\"step_calls\":%llu,\"step_ns\":%llu,\"reset_calls\":%llu,\"reset_ns\":%llu,\"commit_step_calls\":%llu,\"commit_step_ns\":%llu,\"commit_reset_calls\":%llu,\"commit_reset_ns\":%llu}\n",sqlite3_libversion(),atomic_load(&calls[0]),atomic_load(&wall[0]),atomic_load(&calls[1]),atomic_load(&wall[1]),atomic_load(&calls[2]),atomic_load(&wall[2]),atomic_load(&calls[3]),atomic_load(&wall[3]));
 if(fclose(file))fprintf(stderr,"cause API observer output close failed\n");
}
