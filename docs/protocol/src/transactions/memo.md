# Transaction Memo

The transaction-level memo field is optional, and is present _if and only if_
the transaction creates note outputs visible to the sender or receiver. A
consensus rule rejects memos on transactions with no note outputs, and rejects
output-bearing transactions that omit the memo when one is required.

## Memo Plaintext

The plaintext of the memo contains:

* a return address (80 bytes for Penumbra addresses)
* a text string that is 432 bytes in length

## Privacy

The transaction-level encrypted memo is visible only to the sender and receiver(s) of the transaction.

Each memo is encrypted using the *Memo Key*, a symmetric ChaCha20-Poly1305 key
generated randomly as described [here](../addresses_keys/transaction_crypto.md#random-memo-key).
The Memo Key is then encrypted using per-output payload data from the
output-bearing supported actions: `Transfer`, `Split`, `Consolidate`, and the
change output of `ShieldedIcs20Withdrawal`.
