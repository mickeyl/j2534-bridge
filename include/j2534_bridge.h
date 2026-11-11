#ifndef J2534_BRIDGE_H
#define J2534_BRIDGE_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

/* ABI v1. Handles are owned by the caller and must be serialized by the caller.
 * Only create/destroy/start/request below may access them. No vendor DLL runs
 * in the calling process. timeout_ms is the maximum response wait (1..60000).
 * create returns NULL on invalid timeout/allocation panic.
 * start/request return an owned UTF-8 JSON Response from protocol.rs, without
 * the IPC id envelope: {"status":"ok","data":...} or
 * {"status":"error","code":...,"message":...}. Transport/argument errors
 * have code -1. NULL means an allocation/serialization panic.
 * All returned strings MUST be freed with j2534_string_free, never free().
 * request_json is a NUL-terminated UTF-8 Request, e.g. {"method":"EnumerateDevices"}.
 * Destroy stops/reaps the helper. NULL is accepted by destroy/string_free.
 * A non-NULL handle/string must be a live pointer returned by this API.
 */
typedef struct J2534Client J2534Client;
uint32_t j2534_abi_version(void);
J2534Client* j2534_client_create(uint32_t timeout_ms);
void j2534_client_destroy(J2534Client* client);
char* j2534_client_start(J2534Client* client, uint8_t bitness);
char* j2534_client_request(J2534Client* client, const char* request_json);
void j2534_string_free(char* text);

#ifdef __cplusplus
}
#endif
#endif
