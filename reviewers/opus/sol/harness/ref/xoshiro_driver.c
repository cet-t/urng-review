/* Driver for the official prng.di.unimi.it reference sources.
 * Build: gcc -O2 -DSRC="\"<path to official .c>\"" -DW=32|64 -DNS=<state words> [-DHAS_JUMP] [-DHAS_LONG_JUMP]
 * Usage: driver <n> <s0> <s1> ... (hex state words)
 * Prints n outputs, then the state after jump() and after long_jump(). */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include SRC

int main(int argc, char **argv) {
    int n = atoi(argv[1]);
    for (int i = 0; i < NS; i++) s[i] = (uint64_t)strtoull(argv[2 + i], NULL, 16);
    for (int i = 0; i < n; i++) printf("%llx\n", (unsigned long long)next());
#ifdef HAS_JUMP
    jump();
    printf("J");
    for (int i = 0; i < NS; i++) printf(" %llx", (unsigned long long)s[i]);
    printf("\n");
#endif
#ifdef HAS_LONG_JUMP
    long_jump();
    printf("L");
    for (int i = 0; i < NS; i++) printf(" %llx", (unsigned long long)s[i]);
    printf("\n");
#endif
    return 0;
}
