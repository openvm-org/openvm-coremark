/*
 * Minimal CoreMark port for this workspace.
 *
 * This is currently just enough to compile/link CoreMark; it does not provide
 * real time measurement or output. We'll replace stubs with OpenVM-specific
 * implementations later.
 */

#include "core_portme.h"
#include "../coremark/coremark.h"

#include <stdarg.h>

/* Seed values (used when SEED_METHOD == SEED_VOLATILE) */
volatile ee_s32 seed1_volatile = 0x0;
volatile ee_s32 seed2_volatile = 0x0;
volatile ee_s32 seed3_volatile = 0x66;
volatile ee_s32 seed4_volatile = ITERATIONS;
volatile ee_s32 seed5_volatile = 0;

ee_u32 default_num_contexts = 1;

/* Very small "timer" stub */
static CORE_TICKS start_ticks = 0;
static CORE_TICKS stop_ticks = 0;

void start_time(void) { start_ticks = 0; }
void stop_time(void) { stop_ticks = 1; }
CORE_TICKS get_time(void) { return (CORE_TICKS)(stop_ticks - start_ticks); }

secs_ret time_in_secs(CORE_TICKS ticks) {
  /* No real time source yet; just return ticks. */
  return (secs_ret)ticks;
}

void portable_init(core_portable *p, int *argc, char *argv[]) {
  (void)argc;
  (void)argv;
  p->portable_id = 1;
}

void portable_fini(core_portable *p) { p->portable_id = 0; }

int ee_printf(const char *fmt, ...) {
  (void)fmt;
  return 0;
}
