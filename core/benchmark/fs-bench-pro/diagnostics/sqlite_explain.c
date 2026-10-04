/* Read-only native SQLite planner diagnostic, never a performance observer. */
#include <sqlite3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int run(sqlite3 *db,const char *prefix,const char *sql){
 size_t n=strlen(prefix)+strlen(sql)+1;char *text=malloc(n);if(!text)return 1;
 snprintf(text,n,"%s%s",prefix,sql);sqlite3_stmt *stmt=NULL;
 int rc=sqlite3_prepare_v2(db,text,-1,&stmt,NULL);free(text);
 if(rc!=SQLITE_OK){fprintf(stderr,"prepare: %s\n",sqlite3_errmsg(db));return 1;}
 printf("SECTION\t%s\n",prefix);
 while((rc=sqlite3_step(stmt))==SQLITE_ROW){
  for(int i=0;i<sqlite3_column_count(stmt);i++){if(i)putchar('\t');
   const unsigned char *value=sqlite3_column_text(stmt,i);printf("%s",value?(const char*)value:"NULL");}
  putchar('\n');
 }
 sqlite3_finalize(stmt);return rc!=SQLITE_DONE;
}
int main(int argc,char **argv){
 if(argc!=4){fprintf(stderr,"DB baseline|candidate SQL_FILE required\n");return 1;}
 FILE *file=fopen(argv[3],"rb");if(!file)return 1;
 if(fseek(file,0,SEEK_END)){fclose(file);return 1;}long len=ftell(file);
 if(len<1||len>1000000){fclose(file);return 1;}rewind(file);
 char *sql=malloc((size_t)len+1);if(!sql){fclose(file);return 1;}
 if(fread(sql,1,(size_t)len,file)!=(size_t)len){free(sql);fclose(file);return 1;}sql[len]=0;fclose(file);
 sqlite3 *db=NULL;int rc=sqlite3_open_v2(argv[1],&db,SQLITE_OPEN_READONLY|SQLITE_OPEN_NOMUTEX,NULL);
 if(rc!=SQLITE_OK){free(sql);if(db)sqlite3_close(db);return 1;}
 rc=sqlite3_exec(db,"PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY",NULL,NULL,NULL);
 if(rc==SQLITE_OK&&!strcmp(argv[2],"baseline"))rc=sqlite3_exec(db,"CREATE TEMP TABLE layerfs_read_scope(save_id INTEGER NOT NULL,publication INTEGER NOT NULL);INSERT INTO layerfs_read_scope VALUES(0,9223372036854775807)",NULL,NULL,NULL);
 printf("VERSION\t%s\n",sqlite3_libversion());
 int fail=rc!=SQLITE_OK;if(!fail)fail=run(db,"EXPLAIN QUERY PLAN ",sql)|run(db,"EXPLAIN ",sql);
 free(sql);if(sqlite3_close(db)!=SQLITE_OK)fail=1;return fail;
}
