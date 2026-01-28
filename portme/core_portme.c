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

/* Implemented in Rust; routes output through OpenVM. */
extern void coremark_putchar(unsigned char c);

static void ee_putc(char c) { coremark_putchar((unsigned char)c); }

static void ee_puts(const char *s) {
  if (!s)
    s = "(null)";
  while (*s)
    ee_putc(*s++);
}

static void ee_put_uint(unsigned long v, unsigned base, int uppercase,
                        int width, int zero_pad) {
  char buf[32];
  int i = 0;
  const char *digits = uppercase ? "0123456789ABCDEF" : "0123456789abcdef";

  if (base < 2)
    base = 10;

  do {
    buf[i++] = digits[v % base];
    v /= base;
  } while (v && i < (int)sizeof(buf));

  while (i < width)
    buf[i++] = (char)(zero_pad ? '0' : ' ');

  while (i--)
    ee_putc(buf[i]);
}

static void ee_put_int(long v, int width, int zero_pad) {
  if (v < 0) {
    ee_putc('-');
    ee_put_uint((unsigned long)(-v), 10, 0, width, zero_pad);
  } else {
    ee_put_uint((unsigned long)v, 10, 0, width, zero_pad);
  }
}

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
  va_list args;
  int n = 0;

  va_start(args, fmt);
  while (*fmt) {
    if (*fmt != '%') {
      ee_putc(*fmt++);
      n++;
      continue;
    }
    fmt++; /* skip '%' */

    /* flags */
    int zero_pad = 0;
    if (*fmt == '0') {
      zero_pad = 1;
      fmt++;
    }

    /* width */
    int width = 0;
    while (*fmt >= '0' && *fmt <= '9') {
      width = width * 10 + (*fmt - '0');
      fmt++;
    }

    /* length */
    int long_mod = 0;
    if (*fmt == 'l') {
      long_mod = 1;
      fmt++;
    }

    char spec = *fmt ? *fmt++ : '\0';
    switch (spec) {
    case '%':
      ee_putc('%');
      n++;
      break;
    case 'c': {
      int c = va_arg(args, int);
      ee_putc((char)c);
      n++;
      break;
    }
    case 's': {
      const char *s = va_arg(args, const char *);
      ee_puts(s);
      /* n is best-effort; we don't count string length here */
      break;
    }
    case 'd':
    case 'i': {
      long v = long_mod ? va_arg(args, long) : va_arg(args, int);
      ee_put_int(v, width, zero_pad);
      break;
    }
    case 'u': {
      unsigned long v =
          long_mod ? va_arg(args, unsigned long) : va_arg(args, unsigned int);
      ee_put_uint(v, 10, 0, width, zero_pad);
      break;
    }
    case 'x':
    case 'X': {
      unsigned long v =
          long_mod ? va_arg(args, unsigned long) : va_arg(args, unsigned int);
      ee_put_uint(v, 16, (spec == 'X'), width, zero_pad);
      break;
    }
    default:
      /* Unknown specifier: print it literally. */
      ee_putc('%');
      ee_putc(spec ? spec : '?');
      n += 2;
      break;
    }
  }
  va_end(args);
  return n;
}
