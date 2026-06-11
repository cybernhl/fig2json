#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

char *fig2json_convert(const uint8_t *data, uintptr_t len);

void fig2json_free_string(char *s);
