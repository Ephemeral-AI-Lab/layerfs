/* Diagnostic proof-copy adapter: original measured MEMORY database is untouched.
 * Convert only the independent byte copy's journal header for the production
 * read-only verifier, which requires WAL. No application SQL/data mutation.
 */
#include <sqlite3.h>
#include <stdio.h>
#include <string.h>
int main(int argc,char **argv){
 if(argc!=2)return 2;
 sqlite3 *db=0;sqlite3_stmt *stmt=0;
 if(sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READWRITE,0)!=SQLITE_OK)return 3;
 int rc=sqlite3_prepare_v2(db,"PRAGMA journal_mode=WAL",-1,&stmt,0);
 if(rc!=SQLITE_OK||sqlite3_step(stmt)!=SQLITE_ROW||!sqlite3_column_text(stmt,0)||strcmp((const char*)sqlite3_column_text(stmt,0),"wal"))return 4;
 if(sqlite3_finalize(stmt)!=SQLITE_OK||sqlite3_close(db)!=SQLITE_OK)return 5;
 printf("{\"status\":\"PASS\",\"sqlite_version\":\"%s\",\"journal_mode\":\"wal\",\"scope\":\"independent proof byte-copy header only\"}\n",sqlite3_libversion());return 0;
}
