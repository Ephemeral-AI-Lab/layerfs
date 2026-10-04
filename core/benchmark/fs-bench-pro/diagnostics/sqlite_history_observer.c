/* First-party fixed-count qualification observer; original SQLite/VFS operations. */
#define LAYERFS_COMBINED_OBSERVER 1
#define CLASSES 4096
#define opens trace_opens
#include "sqlite_work.c"
#undef opens
#include "sqlite_close_observer.c"
