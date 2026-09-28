/* A stand-in for mxbserver, for testing remote.sh on a CI runner: an ELF (so /proc/<pid>/exe
 * is itself, as the real server's is) that reads --config, --observe and --duration like the
 * server does and answers /readyz.
 *
 *   config contains "invalid"      -> refuses to start (exit 2), like a config the server rejects
 *   config contains "never_ready"  -> runs but never listens, so /readyz never answers
 *   --duration N                   -> exits 0 after a second (the check remote.sh runs)
 *   otherwise                      -> serves 200 on the observe port until SIGINT
 */
#include <arpa/inet.h>
#include <netinet/in.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static volatile sig_atomic_t stop = 0;
static void on_int(int sig) { (void)sig; stop = 1; }

static int port_of(const char *addr) {
    const char *colon = strrchr(addr, ':');
    return colon ? atoi(colon + 1) : 0;
}

int main(int argc, char **argv) {
    const char *config = NULL, *observe = NULL;
    int duration = -1;
    for (int i = 1; i + 1 < argc; i++) {
        if (!strcmp(argv[i], "--config")) config = argv[++i];
        else if (!strcmp(argv[i], "--observe")) observe = argv[++i];
        else if (!strcmp(argv[i], "--duration")) duration = atoi(argv[++i]);
    }
    if (!config) { fprintf(stderr, "no --config\n"); return 2; }
    FILE *f = fopen(config, "r");
    if (!f) { perror(config); return 2; }
    static char text[65536];
    size_t n = fread(text, 1, sizeof text - 1, f);
    text[n] = 0;
    fclose(f);
    if (strstr(text, "invalid")) { fprintf(stderr, "config: invalid value\n"); return 2; }
    printf("stub server: config %s ok\n", config);
    fflush(stdout);
    if (duration >= 0) { sleep(1); return 0; }

    static char from_file[64] = "";
    if (!observe) {
        const char *o = strstr(text, "observe = \"");
        if (o && sscanf(o, "observe = \"%63[^\"]\"", from_file) == 1) observe = from_file;
    }
    /* No SA_RESTART: SIGINT must break a blocked accept()/pause(), as it stops the real server. */
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = on_int;
    sigaction(SIGINT, &sa, NULL);
    sigaction(SIGTERM, &sa, NULL);
    if (strstr(text, "never_ready") || !observe) {
        while (!stop) pause();
        return 0;
    }
    int s = socket(AF_INET, SOCK_STREAM, 0);
    int one = 1;
    setsockopt(s, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_port = htons(port_of(observe));
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(s, (struct sockaddr *)&a, sizeof a) || listen(s, 16)) { perror("bind"); return 1; }
    while (!stop) {
        int c = accept(s, NULL, NULL);
        if (c < 0) continue;
        char buf[512];
        if (read(c, buf, sizeof buf) < 0) { close(c); continue; }
        const char *r = "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"ready\":true}";
        if (write(c, r, strlen(r)) < 0) { /* the client went away */ }
        close(c);
    }
    close(s);
    return 0;
}
