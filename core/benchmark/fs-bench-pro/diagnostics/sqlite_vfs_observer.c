/* First-party diagnostic VFS delegates every operation to the prior default.
 * Public sqlite3_vfs/sqlite3_io_methods ABI; no dependency implementation copy.
 * Fixed counters only. Same arguments, return, optional capabilities and iVersion.
 */
#include <sqlite3.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static sqlite3_vfs proxy,*original;
static _Atomic unsigned long long opens[4],reads[4],read_bytes[4],read_ns[4],writes[4],write_bytes[4],write_ns[4],syncs[4],sync_ns[4],closes[4],close_ns[4],flags_count[32],unknown_flags,live,close_errors;
#ifndef LAYERFS_SQL_CALL_SCOPE
#define LAYERFS_SQL_CALL_SCOPE 1
static _Thread_local int cause_sql_scope;
#endif
/* Caller attribution only, not an assertion about device bytes or syscall cost. */
struct ScopeWork { _Atomic unsigned long long writes,bytes,write_ns,syncs,sync_ns,truncates,truncate_ns,hints,hint_ns,hint_bytes,errors; };
static struct ScopeWork scopes[3][4];
struct File{sqlite3_file base;sqlite3_file *inner;int kind;sqlite3_io_methods methods;};
static size_t offset(void){return (sizeof(struct File)+_Alignof(max_align_t)-1)&~((size_t)_Alignof(max_align_t)-1);}
static struct File *file(sqlite3_file *p){return (struct File*)p;}
static uint64_t now(void){struct timespec t;if(clock_gettime(CLOCK_MONOTONIC_RAW,&t))abort();return (uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec;}
static int close_file(sqlite3_file *p){struct File*f=file(p);uint64_t start=now();int r=f->inner->pMethods->xClose(f->inner);atomic_fetch_add(&closes[f->kind],1);atomic_fetch_add(&close_ns[f->kind],now()-start);atomic_fetch_sub(&live,1);if(r)atomic_fetch_add(&close_errors,1);f->base.pMethods=0;return r;}
static int read_file(sqlite3_file*p,void*b,int n,sqlite3_int64 at){struct File*f=file(p);uint64_t t=now();int r=f->inner->pMethods->xRead(f->inner,b,n,at);atomic_fetch_add(&read_ns[f->kind],now()-t);atomic_fetch_add(&reads[f->kind],1);atomic_fetch_add(&read_bytes[f->kind],n);return r;}
static int write_file(sqlite3_file*p,const void*b,int n,sqlite3_int64 at){struct File*f=file(p);struct ScopeWork*w=&scopes[cause_sql_scope][f->kind];uint64_t t=now();int r=f->inner->pMethods->xWrite(f->inner,b,n,at);uint64_t elapsed=now()-t;atomic_fetch_add(&write_ns[f->kind],elapsed);atomic_fetch_add(&writes[f->kind],1);atomic_fetch_add(&write_bytes[f->kind],n);atomic_fetch_add(&w->writes,1);atomic_fetch_add(&w->bytes,n);atomic_fetch_add(&w->write_ns,elapsed);if(r)atomic_fetch_add(&w->errors,1);return r;}
static int sync_file(sqlite3_file*p,int flag){struct File*f=file(p);struct ScopeWork*w=&scopes[cause_sql_scope][f->kind];uint64_t t=now();int r=f->inner->pMethods->xSync(f->inner,flag);uint64_t elapsed=now()-t;atomic_fetch_add(&sync_ns[f->kind],elapsed);atomic_fetch_add(&syncs[f->kind],1);atomic_fetch_add(&w->syncs,1);atomic_fetch_add(&w->sync_ns,elapsed);if(r)atomic_fetch_add(&w->errors,1);if(flag>=0&&flag<32)atomic_fetch_add(&flags_count[flag],1);else atomic_fetch_add(&unknown_flags,1);return r;}
static int truncate_file(sqlite3_file*p,sqlite3_int64 n){struct File*f=file(p);struct ScopeWork*w=&scopes[cause_sql_scope][f->kind];uint64_t t=now();int r=f->inner->pMethods->xTruncate(f->inner,n);atomic_fetch_add(&w->truncates,1);atomic_fetch_add(&w->truncate_ns,now()-t);if(r)atomic_fetch_add(&w->errors,1);return r;}
static int size_file(sqlite3_file*p,sqlite3_int64*n){struct File*f=file(p);return f->inner->pMethods->xFileSize(f->inner,n);}
static int lock_file(sqlite3_file*p,int n){struct File*f=file(p);return f->inner->pMethods->xLock(f->inner,n);}
static int unlock_file(sqlite3_file*p,int n){struct File*f=file(p);return f->inner->pMethods->xUnlock(f->inner,n);}
static int reserved_file(sqlite3_file*p,int*n){struct File*f=file(p);return f->inner->pMethods->xCheckReservedLock(f->inner,n);}
static int control_file(sqlite3_file*p,int n,void*b){struct File*f=file(p);if(n!=SQLITE_FCNTL_SIZE_HINT)return f->inner->pMethods->xFileControl(f->inner,n,b);struct ScopeWork*w=&scopes[cause_sql_scope][f->kind];sqlite3_int64 bytes=b?*(sqlite3_int64*)b:0;uint64_t t=now();int r=f->inner->pMethods->xFileControl(f->inner,n,b);atomic_fetch_add(&w->hints,1);atomic_fetch_add(&w->hint_ns,now()-t);if(bytes>0)atomic_fetch_add(&w->hint_bytes,(uint64_t)bytes);if(r!=SQLITE_OK&&r!=SQLITE_NOTFOUND)atomic_fetch_add(&w->errors,1);return r;}
static int sector_file(sqlite3_file*p){struct File*f=file(p);return f->inner->pMethods->xSectorSize(f->inner);}
static int device_file(sqlite3_file*p){struct File*f=file(p);return f->inner->pMethods->xDeviceCharacteristics(f->inner);}
static int map_file(sqlite3_file*p,int a,int b,int c,void volatile**d){struct File*f=file(p);return f->inner->pMethods->xShmMap(f->inner,a,b,c,d);}
static int shm_lock(sqlite3_file*p,int a,int b,int c){struct File*f=file(p);return f->inner->pMethods->xShmLock(f->inner,a,b,c);}
static void barrier_file(sqlite3_file*p){struct File*f=file(p);f->inner->pMethods->xShmBarrier(f->inner);}
static int unmap_file(sqlite3_file*p,int a){struct File*f=file(p);return f->inner->pMethods->xShmUnmap(f->inner,a);}
static int fetch_file(sqlite3_file*p,sqlite3_int64 a,int b,void**c){struct File*f=file(p);return f->inner->pMethods->xFetch(f->inner,a,b,c);}
static int unfetch_file(sqlite3_file*p,sqlite3_int64 a,void*b){struct File*f=file(p);return f->inner->pMethods->xUnfetch(f->inner,a,b);}
static int open_file(sqlite3_vfs*v,sqlite3_filename name,sqlite3_file*p,int flag,int*out){
 (void)v;struct File*f=file(p);f->inner=(sqlite3_file*)((char*)p+offset());
 f->kind=flag&SQLITE_OPEN_WAL?1:flag&SQLITE_OPEN_MAIN_DB?0:flag&SQLITE_OPEN_MAIN_JOURNAL?2:3;
 int r=original->xOpen(original,name,f->inner,flag,out);
 if(f->inner->pMethods){int version=f->inner->pMethods->iVersion;if(version<1||version>3){f->inner->pMethods->xClose(f->inner);return SQLITE_CANTOPEN;}
  const sqlite3_io_methods *m=f->inner->pMethods;memset(&f->methods,0,sizeof(f->methods));f->methods.iVersion=version;
  f->methods.xClose=m->xClose?close_file:0;f->methods.xRead=m->xRead?read_file:0;f->methods.xWrite=m->xWrite?write_file:0;f->methods.xTruncate=m->xTruncate?truncate_file:0;f->methods.xSync=m->xSync?sync_file:0;f->methods.xFileSize=m->xFileSize?size_file:0;f->methods.xLock=m->xLock?lock_file:0;f->methods.xUnlock=m->xUnlock?unlock_file:0;f->methods.xCheckReservedLock=m->xCheckReservedLock?reserved_file:0;f->methods.xFileControl=m->xFileControl?control_file:0;f->methods.xSectorSize=m->xSectorSize?sector_file:0;f->methods.xDeviceCharacteristics=m->xDeviceCharacteristics?device_file:0;
  if(version>=2){f->methods.xShmMap=m->xShmMap?map_file:0;f->methods.xShmLock=m->xShmLock?shm_lock:0;f->methods.xShmBarrier=m->xShmBarrier?barrier_file:0;f->methods.xShmUnmap=m->xShmUnmap?unmap_file:0;}
  if(version>=3){f->methods.xFetch=m->xFetch?fetch_file:0;f->methods.xUnfetch=m->xUnfetch?unfetch_file:0;}
  f->base.pMethods=&f->methods;atomic_fetch_add(&opens[f->kind],1);atomic_fetch_add(&live,1);}
 return r;
}
static int delete_vfs(sqlite3_vfs*v,const char*n,int s){(void)v;return original->xDelete(original,n,s);}
static int access_vfs(sqlite3_vfs*v,const char*n,int f,int*r){(void)v;return original->xAccess(original,n,f,r);}
static int path_vfs(sqlite3_vfs*v,const char*n,int s,char*r){(void)v;return original->xFullPathname(original,n,s,r);}
static void *dlopen_vfs(sqlite3_vfs*v,const char*n){(void)v;return original->xDlOpen(original,n);}
static void dlerror_vfs(sqlite3_vfs*v,int n,char*r){(void)v;original->xDlError(original,n,r);}
static void (*dlsym_vfs(sqlite3_vfs*v,void*h,const char*n))(void){(void)v;return original->xDlSym(original,h,n);}
static void dlclose_vfs(sqlite3_vfs*v,void*h){(void)v;original->xDlClose(original,h);}
static int random_vfs(sqlite3_vfs*v,int n,char*r){(void)v;return original->xRandomness(original,n,r);}
static int sleep_vfs(sqlite3_vfs*v,int n){(void)v;return original->xSleep(original,n);}
static int time_vfs(sqlite3_vfs*v,double*r){(void)v;return original->xCurrentTime(original,r);}
static int error_vfs(sqlite3_vfs*v,int n,char*r){(void)v;return original->xGetLastError(original,n,r);}
static int time64_vfs(sqlite3_vfs*v,sqlite3_int64*r){(void)v;return original->xCurrentTimeInt64(original,r);}
static int setcall_vfs(sqlite3_vfs*v,const char*n,sqlite3_syscall_ptr p){(void)v;return original->xSetSystemCall(original,n,p);}
static sqlite3_syscall_ptr getcall_vfs(sqlite3_vfs*v,const char*n){(void)v;return original->xGetSystemCall(original,n);}
static const char *nextcall_vfs(sqlite3_vfs*v,const char*n){(void)v;return original->xNextSystemCall(original,n);}
int cause_vfs_initialize(void){
 if(original)return SQLITE_MISUSE;original=sqlite3_vfs_find(0);
 if(!original||original->iVersion<1||original->iVersion>3||original->szOsFile>INT_MAX-(int)offset())return SQLITE_CANTOPEN;
 proxy=*original;proxy.zName="layerfs-cause-delegate";proxy.pNext=0;proxy.szOsFile=original->szOsFile+(int)offset();
 proxy.xOpen=open_file;proxy.xDelete=delete_vfs;proxy.xAccess=access_vfs;proxy.xFullPathname=path_vfs;
 proxy.xDlOpen=original->xDlOpen?dlopen_vfs:0;proxy.xDlError=original->xDlError?dlerror_vfs:0;proxy.xDlSym=original->xDlSym?dlsym_vfs:0;proxy.xDlClose=original->xDlClose?dlclose_vfs:0;
 proxy.xRandomness=random_vfs;proxy.xSleep=sleep_vfs;proxy.xCurrentTime=time_vfs;proxy.xGetLastError=original->xGetLastError?error_vfs:0;
 proxy.xCurrentTimeInt64=original->iVersion>=2&&original->xCurrentTimeInt64?time64_vfs:0;
 proxy.xSetSystemCall=original->iVersion>=3&&original->xSetSystemCall?setcall_vfs:0;proxy.xGetSystemCall=original->iVersion>=3&&original->xGetSystemCall?getcall_vfs:0;proxy.xNextSystemCall=original->iVersion>=3&&original->xNextSystemCall?nextcall_vfs:0;
 return sqlite3_vfs_register(&proxy,1);
}
__attribute__((destructor))static void report(void){
 if(!original)return;const char*path=getenv("LAYERFS_CAUSE_VFS_LOG");if(!path)return;FILE*f=fopen(path,"wx");if(!f){fprintf(stderr,"cause VFS observer output refused\n");return;}
 fprintf(f,"{\"kind\":\"delegated-vfs-observer\",\"version\":\"%s\",\"underlying_vfs\":\"%s\",\"vfs_version\":%d,\"underlying_os_file_bytes\":%d,\"observer_file_header_bytes\":%zu,\"live_files\":%llu,\"close_errors\":%llu,\"unknown_sync_flags\":%llu,\"files\":[",sqlite3_libversion(),original->zName,original->iVersion,original->szOsFile,offset(),atomic_load(&live),atomic_load(&close_errors),atomic_load(&unknown_flags));
 const char*names[4]={"main","wal","journal","other"};for(int i=0;i<4;i++){if(i)fputc(',',f);fprintf(f,"{\"class\":\"%s\",\"opens\":%llu,\"reads\":%llu,\"read_requested_bytes\":%llu,\"read_ns\":%llu,\"writes\":%llu,\"write_submitted_bytes\":%llu,\"write_ns\":%llu,\"syncs\":%llu,\"sync_ns\":%llu,\"closes\":%llu,\"close_ns\":%llu}",names[i],atomic_load(&opens[i]),atomic_load(&reads[i]),atomic_load(&read_bytes[i]),atomic_load(&read_ns[i]),atomic_load(&writes[i]),atomic_load(&write_bytes[i]),atomic_load(&write_ns[i]),atomic_load(&syncs[i]),atomic_load(&sync_ns[i]),atomic_load(&closes[i]),atomic_load(&close_ns[i]));}
 fprintf(f,"],\"call_scopes\":[");const char*scope_names[3]={"other","commit_step","checkpoint_step"};
 for(int scope=0;scope<3;scope++)for(int kind=0;kind<4;kind++){struct ScopeWork*w=&scopes[scope][kind];if(scope||kind)fputc(',',f);fprintf(f,"{\"scope\":\"%s\",\"class\":\"%s\",\"writes\":%llu,\"write_submitted_bytes\":%llu,\"write_ns\":%llu,\"syncs\":%llu,\"sync_ns\":%llu,\"truncates\":%llu,\"truncate_ns\":%llu,\"size_hints\":%llu,\"size_hint_ns\":%llu,\"size_hint_bytes\":%llu,\"errors\":%llu}",scope_names[scope],names[kind],atomic_load(&w->writes),atomic_load(&w->bytes),atomic_load(&w->write_ns),atomic_load(&w->syncs),atomic_load(&w->sync_ns),atomic_load(&w->truncates),atomic_load(&w->truncate_ns),atomic_load(&w->hints),atomic_load(&w->hint_ns),atomic_load(&w->hint_bytes),atomic_load(&w->errors));}
 fprintf(f,"],\"sync_flags\":[");for(int i=0;i<32;i++){if(i)fputc(',',f);fprintf(f,"%llu",atomic_load(&flags_count[i]));}fprintf(f,"]}\n");if(fclose(f))fprintf(stderr,"cause VFS observer close failed\n");
}
