# Addresses and Keys

Value transferred on Shieldd is sent to *shielded payment addresses*; these
addresses are derived from *spending keys* through a sequence of intermediate
keys that represent different levels of attenuated capability:

```mermaid
flowchart BT
    subgraph Address
    end;
    subgraph DTK[Detection Key]
    end;
    subgraph IVK[Incoming Viewing Key]
    end;
    subgraph OVK[Outgoing Viewing Key]
    end;
    subgraph FVK[Full Viewing Key]
    end;
    subgraph SK[Spending Key]
    end;
    subgraph SeedPhrase[Seed Phrase]
    end;

    index(address index);
    div{ };

    SeedPhrase --> SK;
    SK --> FVK;
    FVK --> OVK;
    FVK --> IVK;

    index --> div;
    IVK ----> div;
    div --> Address & DTK;
    DTK --> Address;
```

From bottom to top:

- the *seed phrase* is the root key material. Multiple *wallets* - each with
separate spend authority - can be derived from this root seed phrase.
- the *spending key* is the capability representing spending authority for a given wallet;
- the *full viewing key* represents the capability to view all transactions related to the wallet;
- the *outgoing viewing key* represents the capability to view only outgoing transactions, and is used to recover information about previously sent transactions;
- the *incoming viewing key* represents the capability to view only incoming transactions, and is used to scan the block chain for incoming transactions.

Shieldd allows the same account to present multiple, publicly
unlinkable addresses, keyed by an 16-byte *address index*.  Each choice of
address index gives a distinct shielded payment address. Because these
addresses share a common incoming viewing key, the cost of scanning the
blockchain does not increase with the number of addresses in use.

This diagram shows only the user-visible parts of the key hierarchy.
Internally, each of these keys has different components, described in detail in
the [Addresses and Keys](../addresses_keys.md) chapter.

### Privacy Implications

Users should be aware that giving out multiple detection keys to a detection entity can carry a subset of the privacy implications, described in [Addresses and Detection Keys](../addresses_keys/addresses.md#detection-keys).
