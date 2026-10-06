/* External refusal proof only: fail the direct segment's full-sync request.
 * Known fcntl argument classes retain their ABI. Unknown commands fail explicitly.
 * No application/dependency code is patched; never use this in a speed arm.
 */
#include <fcntl.h>
#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static int fail_segment_sync(int fd, int command, ...) {
 if(command==F_FULLFSYNC) {
  char path[1024];
  if(fcntl(fd,F_GETPATH,path)==0) {
   size_t n=strlen(path);
   if(n>=8 && strcmp(path+n-8,".segment")==0) {
    fprintf(stderr,"EXTERNAL_SEGMENT_SYNC_REFUSAL %s\n",path);errno=EIO;return -1;
   }
  }
  return fcntl(fd,command);
 }
 if(command==F_GETFD || command==F_GETFL || command==F_GETOWN || command==F_GETNOSIGPIPE) return fcntl(fd,command);
 va_list args;va_start(args,command);int result;
 switch(command) {
  case F_SETFD:case F_SETFL:case F_SETOWN:case F_RDAHEAD:case F_NOCACHE:case F_SETNOSIGPIPE:case F_TRANSFEREXTENTS:
   {int arg=va_arg(args,int);result=fcntl(fd,command,arg);break;}
  case F_GETLK:case F_SETLK:case F_SETLKW:case F_PREALLOCATE:case F_GETPATH:
   {void *arg=va_arg(args,void*);result=fcntl(fd,command,arg);break;}
  default: fprintf(stderr,"UNSUPPORTED_FCNTL_COMMAND %d\n",command);errno=ENOTSUP;result=-1;
 }
 va_end(args);return result;
}
__attribute__((used)) static struct { const void *replacement;const void *replacee; }
interpose __attribute__((section("__DATA,__interpose"))) = {(const void*)(uintptr_t)&fail_segment_sync,(const void*)(uintptr_t)&fcntl};
