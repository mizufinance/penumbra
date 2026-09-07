#ifndef SHIELDD_H
#define SHIELDD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SHIELDD_ABI_VERSION 1u

typedef struct shieldd_handle shieldd_handle_t;

typedef struct shieldd_buffer {
  uint8_t *data;
  size_t len;
} shieldd_buffer_t;

typedef struct shieldd_result {
  int32_t status;
  shieldd_buffer_t response;
  shieldd_buffer_t error;
} shieldd_result_t;

enum shieldd_status {
  SHIELDD_STATUS_OK = 0,
  SHIELDD_STATUS_INVALID_ARGUMENT = 1,
  SHIELDD_STATUS_FAILED_PRECONDITION = 2,
  SHIELDD_STATUS_INTERNAL = 3,
  SHIELDD_STATUS_PANIC = 4,
  SHIELDD_STATUS_NOT_FOUND = 5,
};

/*
 * Execution methods use the corresponding shieldd.execution_client.v1
 * protobuf request and response unless documented otherwise.
 */
enum shieldd_method {
  SHIELDD_METHOD_INIT_GENESIS = 1,
  SHIELDD_METHOD_BEGIN_BLOCK = 2,
  SHIELDD_METHOD_DEPOSIT = 3,
  SHIELDD_METHOD_CHECK_TX = 4,
  SHIELDD_METHOD_DELIVER_TX = 5,
  SHIELDD_METHOD_END_BLOCK = 6,
  SHIELDD_METHOD_COMMIT = 7,
  SHIELDD_METHOD_ROLLBACK = 8,
  SHIELDD_METHOD_EXPORT_GENESIS = 9,
  SHIELDD_METHOD_GET_COMMITTED_STATE = 10,
  SHIELDD_METHOD_ARCHIVED_NULLIFIER_PROOF = 11,
  SHIELDD_METHOD_APPLY_COMPLIANCE_ACTION = 12,

  /* Read-only queries use IDs starting at 1000000. */

  /* shieldd.core.app.v1.AppParametersRequest/Response */
  SHIELDD_METHOD_QUERY_APP_PARAMETERS = 1000000,

  /*
   * shieldd.core.component.shielded_pool.v1.
   * AssetMetadataByIdRequest/Response
   */
  SHIELDD_METHOD_QUERY_ASSET_METADATA_BY_ID = 1000001,

  /*
   * shieldd.core.component.compliance.v1.
   * ComplianceAssetStatusRequest/Response
   */
  SHIELDD_METHOD_QUERY_COMPLIANCE_ASSET_STATUS = 1000002,

  /*
   * shieldd.core.component.compliance.v1.
   * ComplianceBatchMerkleProofsRequest/Response
   */
  SHIELDD_METHOD_QUERY_COMPLIANCE_BATCH_MERKLE_PROOFS = 1000003,

  /*
   * shieldd.core.component.compliance.v1.
   * ComplianceUserLeafRequest/Response
   */
  SHIELDD_METHOD_QUERY_COMPLIANCE_USER_LEAF = 1000004,

  /*
   * Accepts penumbra.cnidarium.v1.KeyValueRequest and returns
   * penumbra.cnidarium.v1.KeyValueResponse. The frontend uses this existing
   * query with SCT state keys.
   */
  SHIELDD_METHOD_QUERY_KEY_VALUE = 1000005,

  /*
   * Accepts shieldd.core.component.compact_block.v1.
   * CompactBlockRangeRequest with keep_alive=false. Returns zero or more
   * protobuf-varint-delimited CompactBlockRangeResponse messages.
   */
  SHIELDD_METHOD_QUERY_COMPACT_BLOCK_RANGE = 1000006,

  /*
   * shieldd.core.component.sct.v1.
   * NullifierWindowRequest/Response
   */
  SHIELDD_METHOD_QUERY_NULLIFIER_WINDOW = 1000007,
};

uint32_t shieldd_abi_version(void);

/*
 * Opens RocksDB and creates one persistent execution handle.
 *
 * db_path is UTF-8 and is not required to be NUL-terminated. out_handle is
 * set to NULL on failure. The caller must eventually pass a successful handle
 * to shieldd_close after all concurrent calls have completed.
 */
shieldd_result_t shieldd_open(const uint8_t *db_path, size_t db_path_len,
                              shieldd_handle_t **out_handle);

/* Opens Shieldd with retired-generation witness storage enabled. */
shieldd_result_t shieldd_open_with_generation_packs(
    const uint8_t *db_path, size_t db_path_len,
    const uint8_t *generation_pack_path, size_t generation_pack_path_len,
    shieldd_handle_t **out_handle);

/*
 * Executes one protobuf request. Calls sharing a handle are serialized.
 *
 * request may be NULL only when request_len is zero. On success, response is
 * the protobuf encoding for method. On failure, error is a UTF-8 message.
 */
shieldd_result_t shieldd_call(shieldd_handle_t *handle, uint32_t method,
                              const uint8_t *request, size_t request_len);

/*
 * Releases Cnidarium/RocksDB and consumes handle. The caller must ensure all
 * calls have completed and no new calls can begin before calling this function.
 * The handle is invalid afterward regardless of the returned status.
 */
shieldd_result_t shieldd_close(shieldd_handle_t *handle);

/*
 * Frees a response or error buffer returned by Shieldd and clears the struct.
 * Passing NULL or an already-cleared buffer is safe.
 */
void shieldd_buffer_free(shieldd_buffer_t *buffer);

#ifdef __cplusplus
}
#endif

#endif
