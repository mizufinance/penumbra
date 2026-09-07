#include "shieldd.h"

#include <stdio.h>
#include <string.h>

static void print_error(const shieldd_result_t *result) {
  if (result->error.data != NULL && result->error.len != 0) {
    fwrite(result->error.data, 1, result->error.len, stderr);
    fputc('\n', stderr);
  }
}

static void free_result(shieldd_result_t *result) {
  shieldd_buffer_free(&result->response);
  shieldd_buffer_free(&result->error);
}

int main(int argc, char **argv) {
  shieldd_handle_t *handle = NULL;
  shieldd_result_t result;

  if (argc != 2) {
    fprintf(stderr, "usage: %s DB_PATH\n", argv[0]);
    return 2;
  }
  if (shieldd_abi_version() != SHIELDD_ABI_VERSION) {
    fputs("unexpected Shieldd ABI version\n", stderr);
    return 3;
  }

  result = shieldd_open((const uint8_t *)argv[1], strlen(argv[1]), &handle);
  if (result.status != SHIELDD_STATUS_OK) {
    print_error(&result);
    free_result(&result);
    return 4;
  }
  free_result(&result);

  result = shieldd_call(handle, SHIELDD_METHOD_ROLLBACK, NULL, 0);
  if (result.status != SHIELDD_STATUS_OK) {
    print_error(&result);
    free_result(&result);
    result = shieldd_close(handle);
    free_result(&result);
    return 5;
  }
  free_result(&result);

  result =
      shieldd_call(handle, SHIELDD_METHOD_GET_COMMITTED_STATE, NULL, 0);
  if (result.status != SHIELDD_STATUS_FAILED_PRECONDITION) {
    print_error(&result);
    free_result(&result);
    result = shieldd_close(handle);
    free_result(&result);
    return 6;
  }
  free_result(&result);

  result =
      shieldd_call(handle, SHIELDD_METHOD_QUERY_APP_PARAMETERS, NULL, 0);
  if (result.status != SHIELDD_STATUS_FAILED_PRECONDITION) {
    print_error(&result);
    free_result(&result);
    result = shieldd_close(handle);
    free_result(&result);
    return 7;
  }
  free_result(&result);

  result =
      shieldd_call(handle, SHIELDD_METHOD_QUERY_NULLIFIER_WINDOW, NULL, 0);
  if (result.status != SHIELDD_STATUS_FAILED_PRECONDITION) {
    print_error(&result);
    free_result(&result);
    result = shieldd_close(handle);
    free_result(&result);
    return 8;
  }
  free_result(&result);

  result = shieldd_close(handle);
  if (result.status != SHIELDD_STATUS_OK) {
    print_error(&result);
    free_result(&result);
    return 9;
  }
  free_result(&result);
  return 0;
}
