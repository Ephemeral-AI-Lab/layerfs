/* D2 -- what the step's transaction is made of, priced on the engine's own terms.
 *
 * The in-product probe (sqlite/step_probe.rs) reads the per-step figures on the
 * product's own connection, but it cannot separate the parts of one statement:
 * rusqlite hands `Connection::execute` a SQL text and a parameter list, and parse,
 * bind and step happen inside one call. This probe replays the same SQL, with the
 * same parameter shapes, over a copy of the measured run's own Store under the
 * declared pragma profile, and prices each part separately. Arms are interleaved
 * round-robin in one process so machine drift is shared.
 *
 *   cc -O2 -o step_cost_probe step_cost_probe.c -lsqlite3
 *   ./step_cost_probe <store-copy.sqlite> <rounds> <body-bytes>
 */
#include <sqlite3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define PACK_APPEND \
  "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = " \
  "(SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves " \
  "WHERE saves.save_id = object_packs.save_id AND publication IS NULL)"
#define NEXT_PACK "SELECT next_pack_id FROM store_policy WHERE id = 1"

static double now_us(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

static void check(int rc, sqlite3 *db, const char *what) {
  if (rc != SQLITE_OK && rc != SQLITE_ROW && rc != SQLITE_DONE) {
    fprintf(stderr, "%s: %s\n", what, sqlite3_errmsg(db));
    exit(1);
  }
}

static void exec(sqlite3 *db, const char *sql) {
  char *err = NULL;
  if (sqlite3_exec(db, sql, NULL, NULL, &err) != SQLITE_OK) {
    fprintf(stderr, "%s: %s\n", sql, err ? err : "?");
    exit(1);
  }
}

/* Page accounting for the full-step arm, so a step price is never quoted without the
 * number of pages the commit that follows it actually wrote. */
static long long step_pages = 0;
static long long step_commits = 0;
static long long step_rows = 0;
static long long step_vm = 0;

static long long cache_write(sqlite3 *db) {
  int cur = 0, hi = 0;
  sqlite3_db_status(db, SQLITE_DBSTATUS_CACHE_WRITE, &cur, &hi, 0);
  return cur;
}

/* One arm: the whole loop for `rounds` iterations, timed. */
/* The body a real append hands over grows on every call, so the row never takes
 * SQLite's in-place overwrite path; the cycle here is D5's (96 KiB to the cap and
 * back), and the length is recomputed per call. */
static int cycle_len(int i, int base_len, int growth, int cap) {
  int span = cap - base_len;
  if (span <= 0) return base_len;
  return base_len + (i * growth) % span;
}

static double run_arm(sqlite3 *db, int arm, int rounds, unsigned char *body, int base_len,
                      int growth, int cap, sqlite3_stmt *append, sqlite3_stmt *next) {
  double t0 = now_us();
  for (int i = 0; i < rounds; i++) {
    int body_len = cycle_len(i, base_len, growth, cap);
    switch (arm) {
      case 0: /* BEGIN IMMEDIATE + ROLLBACK: the transaction open and abandon */
        exec(db, "BEGIN IMMEDIATE");
        exec(db, "ROLLBACK");
        break;
      case 1: /* BEGIN IMMEDIATE + COMMIT: the transaction open and close */
        exec(db, "BEGIN IMMEDIATE");
        exec(db, "COMMIT");
        break;
      case 2: /* deferred BEGIN + COMMIT: no eager RESERVED lock */
        exec(db, "BEGIN");
        exec(db, "COMMIT");
        break;
      case 3: /* prepare the pack UPDATE fresh, then finalize: parse only */
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &s, NULL), db, "prepare");
          sqlite3_finalize(s); }
        break;
      case 4: /* prepare + bind TRANSIENT + reset: parse and copy, no step */
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &s, NULL), db, "prepare");
          sqlite3_bind_int64(s, 1, 1);
          sqlite3_bind_blob(s, 2, body, body_len, SQLITE_TRANSIENT);
          sqlite3_reset(s);
          sqlite3_finalize(s); }
        break;
      case 5: /* prepare + bind STATIC + reset: parse with no copy */
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &s, NULL), db, "prepare");
          sqlite3_bind_int64(s, 1, 1);
          sqlite3_bind_blob(s, 2, body, body_len, SQLITE_STATIC);
          sqlite3_reset(s);
          sqlite3_finalize(s); }
        break;
      case 6: /* bind TRANSIENT alone, on an already prepared statement */
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_TRANSIENT);
        sqlite3_reset(append);
        break;
      case 7: /* bind STATIC alone */
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_STATIC);
        sqlite3_reset(append);
        break;
      case 8: /* the whole step: BEGIN IMMEDIATE, the append, COMMIT */
        exec(db, "BEGIN IMMEDIATE");
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_TRANSIENT);
        check(sqlite3_step(append), db, "append");
        step_rows += sqlite3_changes(db);
        step_vm += sqlite3_stmt_status(append, SQLITE_STMTSTATUS_VM_STEP, 0);
        sqlite3_reset(append);
        { long long before = cache_write(db);
          exec(db, "COMMIT");
          step_pages += cache_write(db) - before;
          step_commits += 1; }
        break;
      case 9: /* the step without the body bind: step cost alone */
        exec(db, "BEGIN IMMEDIATE");
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_STATIC);
        check(sqlite3_step(append), db, "append");
        sqlite3_reset(append);
        exec(db, "COMMIT");
        break;
      case 10: /* the watermark read the step opens with, prepared fresh */
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, NEXT_PACK, -1, &s, NULL), db, "prepare");
          sqlite3_step(s);
          sqlite3_finalize(s); }
        break;
      case 11: /* the same read on a prepared statement: execution alone */
        sqlite3_reset(next);
        sqlite3_step(next);
        break;
      case 12: /* the same step with a STATIC bind: no body copy, same pages */
        exec(db, "BEGIN IMMEDIATE");
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_STATIC);
        check(sqlite3_step(append), db, "append");
        step_rows += sqlite3_changes(db);
        step_vm += sqlite3_stmt_status(append, SQLITE_STMTSTATUS_VM_STEP, 0);
        sqlite3_reset(append);
        { long long before = cache_write(db);
          exec(db, "COMMIT");
          step_pages += cache_write(db) - before;
          step_commits += 1; }
        break;
      case 13: /* a repeat of arm 9: identical code, different position in the round */
        exec(db, "BEGIN IMMEDIATE");
        sqlite3_bind_int64(append, 1, 1);
        sqlite3_bind_blob(append, 2, body, body_len, SQLITE_STATIC);
        check(sqlite3_step(append), db, "append");
        step_rows += sqlite3_changes(db);
        sqlite3_reset(append);
        { long long before = cache_write(db);
          exec(db, "COMMIT");
          step_pages += cache_write(db) - before;
          step_commits += 1; }
        break;
      case 14: /* the whole step with the pack text PARSED FRESH on every call */
        exec(db, "BEGIN IMMEDIATE");
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &s, NULL), db, "prepare");
          sqlite3_bind_int64(s, 1, 1);
          sqlite3_bind_blob(s, 2, body, body_len, SQLITE_TRANSIENT);
          check(sqlite3_step(s), db, "append");
          step_rows += sqlite3_changes(db);
          sqlite3_finalize(s); }
        { long long before = cache_write(db);
          exec(db, "COMMIT");
          step_pages += cache_write(db) - before;
          step_commits += 1; }
        break;
      case 15: /* the whole step, pack text parsed fresh, STATIC bind */
        exec(db, "BEGIN IMMEDIATE");
        { sqlite3_stmt *s = NULL;
          check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &s, NULL), db, "prepare");
          sqlite3_bind_int64(s, 1, 1);
          sqlite3_bind_blob(s, 2, body, body_len, SQLITE_STATIC);
          check(sqlite3_step(s), db, "append");
          step_rows += sqlite3_changes(db);
          sqlite3_finalize(s); }
        { long long before = cache_write(db);
          exec(db, "COMMIT");
          step_pages += cache_write(db) - before;
          step_commits += 1; }
        break;
    }
  }
  return (now_us() - t0) / (double)rounds;
}

static const char *ARM_NAMES[] = {
  "begin_immediate+rollback", "begin_immediate+commit", "begin_deferred+commit",
  "prepare pack UPDATE (fresh)", "prepare+bind TRANSIENT (no step)",
  "prepare+bind STATIC (no step)", "bind TRANSIENT only", "bind STATIC only",
  "full step (BEGIN+append+COMMIT, TRANSIENT)", "full step (STATIC bind)",
  "next_pack (prepare+step+fresh)", "next_pack (step only, prepared)",
  "step with STATIC bind (repeat of 9)",
  "step, prepared handle (repeat of 9)",
  "step, pack text parsed per call (TRANSIENT)",
  "step, pack text parsed per call (STATIC)",
};
#define ARMS ((int)(sizeof(ARM_NAMES) / sizeof(ARM_NAMES[0])))

int main(int argc, char **argv) {
  if (argc < 4) { fprintf(stderr, "usage: %s <store.sqlite> <rounds> <body-bytes>\n", argv[0]); return 2; }
  setvbuf(stdout, NULL, _IONBF, 0);
  const char *path = argv[1];
  int rounds = atoi(argv[2]);
  int body_len = atoi(argv[3]);
  int growth = argc > 4 ? atoi(argv[4]) : 1450;
  int cap = argc > 5 ? atoi(argv[5]) : 262112;
  unsigned char *body = malloc((size_t)cap);
  for (int i = 0; i < body_len; i++) body[i] = (unsigned char)(i * 31 + 7);

  sqlite3 *db = NULL;
  check(sqlite3_open(path, &db), db, "open");
  exec(db, "PRAGMA journal_mode = MEMORY");
  exec(db, "PRAGMA synchronous = OFF");
  exec(db, "PRAGMA temp_store = MEMORY");
  exec(db, "PRAGMA foreign_keys = ON");
  exec(db, "PRAGMA busy_timeout = 0");
  /* The finished Store has no in-flight save, so the append's EXISTS predicate would
   * match nothing and the UPDATE would write no page. One save is put back into the
   * in-flight state the predicate describes (active slot, no publication), which is a
   * legal row under the schema's own CHECK, and the read scope is pointed at it. This
   * is a declared synthetic precondition, not a changed statement. */
  exec(db, "UPDATE saves SET active_slot = 1, publication = NULL"
           " WHERE save_id = (SELECT save_id FROM object_packs ORDER BY pack_id LIMIT 1)");
  exec(db, "CREATE TEMP TABLE IF NOT EXISTS layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)");
  exec(db, "DELETE FROM temp.layerfs_read_scope");
  exec(db, "INSERT INTO temp.layerfs_read_scope SELECT save_id, 0 FROM saves WHERE active_slot = 1");

  sqlite3_stmt *append = NULL, *next = NULL;
  check(sqlite3_prepare_v2(db, PACK_APPEND, -1, &append, NULL), db, "prepare append");
  check(sqlite3_prepare_v2(db, NEXT_PACK, -1, &next, NULL), db, "prepare next");

  printf("store=%s rounds=%d body_bytes=%d growth=%d cap=%d sqlite=%s\n",
         path, rounds, body_len, growth, cap, sqlite3_libversion());
  printf("pack 1 body before: %d bytes\n",
         (int)({ sqlite3_stmt *s; sqlite3_int64 n = -1;
                 if (sqlite3_prepare_v2(db, "SELECT length(data) FROM object_packs WHERE pack_id = 1", -1, &s, NULL) == SQLITE_OK
                     && sqlite3_step(s) == SQLITE_ROW) n = sqlite3_column_int64(s, 0);
                 sqlite3_finalize(s); n; }));

  double total[ARMS];
  for (int a = 0; a < ARMS; a++) total[a] = 0.0;
  int order[ARMS];
  for (int a = 0; a < ARMS; a++) order[a] = a;
  /* Round-robin: every arm is sampled once per round, so drift is shared. */
  for (int r = 0; r < 3; r++) {
    for (int k = 0; k < ARMS; k++) {
      int a = order[k];
      fprintf(stderr, "arm %d round %d\n", a, r);
      total[a] += run_arm(db, a, rounds, body, body_len, growth, cap, append, next);
    }
  }
  printf("%-44s %12s\n", "arm", "us per call");
  for (int a = 0; a < ARMS; a++) printf("%-44s %12.3f\n", ARM_NAMES[a], total[a] / 3.0);
  { sqlite3_stmt *s = NULL;
    if (sqlite3_prepare_v2(db, "SELECT length(data) FROM object_packs WHERE pack_id = 1", -1, &s, NULL) == SQLITE_OK
        && sqlite3_step(s) == SQLITE_ROW)
      printf("pack 1 body after: %lld bytes\n", sqlite3_column_int64(s, 0));
    sqlite3_finalize(s); }
  printf("step arm: %lld commits, %lld rows changed, %lld pages written, %.2f pages/commit, %lld vm steps\n",
         step_commits, step_rows, step_pages,
         step_commits ? (double)step_pages / (double)step_commits : 0.0, step_vm);
  sqlite3_finalize(append);
  sqlite3_finalize(next);
  sqlite3_close(db);
  free(body);
  return 0;
}
