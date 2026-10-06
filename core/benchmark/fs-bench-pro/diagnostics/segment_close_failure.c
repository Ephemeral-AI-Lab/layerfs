/* External checked-close reply-loss proof, never part of a speed arm.
 * Delegate every native close with identical arguments, then make exactly the
 * segment close result uncertain. No fcntl or synchronization call is replaced.
 */
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static int uncertain_segment_close(int fd) {
 char path[1024];int segment=0;
 if(fcntl(fd,F_GETPATH,path)==0) {
  size_t n=strlen(path);segment=n>=8 && strcmp(path+n-8,".segment")==0;
 }
 int result=close(fd);
 if(segment) {fprintf(stderr,"EXTERNAL_SEGMENT_CLOSE_UNCERTAINTY %s original_result=%d\n",path,result);errno=EIO;return -1;}
 return result;
}
__attribute__((used)) static struct { const void *replacement;const void *replacee; }
interpose __attribute__((section("__DATA,__interpose"))) = {(const void*)(uintptr_t)&uncertain_segment_close,(const void*)(uintptr_t)&close};
