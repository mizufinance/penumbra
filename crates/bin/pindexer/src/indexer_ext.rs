pub trait IndexerExt: Sized {
    fn with_default_penumbra_app_views(self, options: &crate::Options) -> Self;
}

impl IndexerExt for cometindex::Indexer {
    fn with_default_penumbra_app_views(self, _options: &crate::Options) -> Self {
        self.with_index(Box::new(crate::block::Block {}))
            .with_index(Box::new(crate::stake::ValidatorSet {}))
            .with_index(Box::new(crate::governance::GovernanceProposals {}))
            .with_index(Box::new(crate::supply::Component::new()))
            .with_index(Box::new(crate::ibc::Component::new()))
            .with_index(Box::new(crate::insights::Component::new()))
    }
}
