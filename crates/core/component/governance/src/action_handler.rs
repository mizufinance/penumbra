use cnidarium_component::ActionHandler;

pub mod validator_vote;

// Note: The ProposalSubmit action handler is defined in `penumbra-app`
// due to it requiring knowledge of all other actions and the `TransactionPlan`,
// located in `penumbra-transaction`.
