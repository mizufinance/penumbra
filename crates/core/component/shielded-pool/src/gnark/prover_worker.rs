use std::{
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
};

use crate::ProofError;

struct ProveRequest<Public, Private, Proof> {
    public: Public,
    private: Private,
    response: SyncSender<Result<Proof, ProofError>>,
}

/// Native state is initialized and dropped on its worker; one request may wait behind it.
pub(crate) struct ProverWorker<Public, Private, Proof> {
    name: &'static str,
    sender: Option<SyncSender<ProveRequest<Public, Private, Proof>>>,
    thread: Option<JoinHandle<()>>,
}

impl<Public: Send + 'static, Private: Send + 'static, Proof: Send + 'static>
    ProverWorker<Public, Private, Proof>
{
    pub fn spawn<State: 'static>(
        name: &'static str,
        initialize: impl FnOnce() -> State + Send + 'static,
        mut execute: impl FnMut(&mut State, Public, Private) -> Result<Proof, ProofError>
            + Send
            + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<ProveRequest<Public, Private, Proof>>(1);
        let thread = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut state = initialize();
                while let Ok(request) = receiver.recv() {
                    let result = execute(&mut state, request.public, request.private);
                    let _ = request.response.send(result);
                }
            })
            .expect("spawn prover worker");
        Self {
            name,
            sender: Some(sender),
            thread: Some(thread),
        }
    }

    pub fn prove(&self, public: Public, private: Private) -> Result<Proof, ProofError> {
        let (response, receiver) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .expect("live prover worker")
            .send(ProveRequest {
                public,
                private,
                response,
            })
            .map_err(|_| self.failure("request channel closed"))?;
        receiver
            .recv()
            .map_err(|_| self.failure("worker exited before replying"))?
    }

    fn failure(&self, reason: &str) -> ProofError {
        ProofError::ProofGenerationFailed(format!("{}: {reason}", self.name))
    }
}

impl<Public, Private, Proof> Drop for ProverWorker<Public, Private, Proof> {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{rc::Rc, sync::mpsc::TrySendError};

    #[test]
    fn concurrent_callers_receive_their_own_responses() {
        let worker = std::sync::Arc::new(ProverWorker::spawn(
            "concurrent-test",
            || 0u32,
            |count, input: u32, ()| {
                *count += 1;
                Ok((input, *count))
            },
        ));
        let callers: Vec<_> = (0..16)
            .map(|input| {
                let worker = worker.clone();
                thread::spawn(move || {
                    let (returned, sequence) = worker.prove(input, ()).unwrap();
                    assert_eq!(returned, input);
                    sequence
                })
            })
            .collect();
        let mut sequences: Vec<_> = callers
            .into_iter()
            .map(|caller| caller.join().unwrap())
            .collect();
        sequences.sort_unstable();
        assert_eq!(sequences, (1..=16).collect::<Vec<_>>());
    }

    #[test]
    fn initialization_failure_closes_the_request_channel() {
        let worker = ProverWorker::spawn(
            "init-failure",
            || panic!("initialization failed"),
            |_: &mut (), (): (), (): ()| Ok(()),
        );
        assert!(worker.prove((), ()).is_err());
    }

    #[test]
    fn native_state_is_created_and_destroyed_on_the_owner() {
        struct State {
            _not_send: Rc<()>,
            destroyed: mpsc::Sender<thread::ThreadId>,
        }
        impl Drop for State {
            fn drop(&mut self) {
                self.destroyed.send(thread::current().id()).unwrap();
            }
        }
        let (destroyed, received) = mpsc::channel();
        let worker = ProverWorker::spawn(
            "owner-test",
            move || State {
                _not_send: Rc::new(()),
                destroyed,
            },
            |_, (), ()| Ok(thread::current().id()),
        );
        let owner = worker.prove((), ()).unwrap();
        assert_ne!(owner, thread::current().id());
        drop(worker);
        assert_eq!(received.recv().unwrap(), owner);
    }

    #[test]
    fn exactly_one_request_can_wait_behind_an_active_request() {
        let (started, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let worker = ProverWorker::spawn(
            "bounded-test",
            || (),
            move |_, public: u8, ()| {
                if public == 1 {
                    started.send(()).unwrap();
                    gate.recv().unwrap();
                }
                Ok(public)
            },
        );
        let sender = worker.sender.as_ref().unwrap();
        let (reply, responses) = mpsc::sync_channel(3);
        sender
            .send(ProveRequest {
                public: 1,
                private: (),
                response: reply.clone(),
            })
            .unwrap();
        ready.recv().unwrap();
        sender
            .try_send(ProveRequest {
                public: 2,
                private: (),
                response: reply.clone(),
            })
            .unwrap();
        assert!(matches!(
            sender.try_send(ProveRequest {
                public: 3,
                private: (),
                response: reply
            }),
            Err(TrySendError::Full(_))
        ));
        release.send(()).unwrap();
        assert_eq!(responses.recv().unwrap().unwrap(), 1);
        assert_eq!(responses.recv().unwrap().unwrap(), 2);
        assert_eq!(worker.prove(3, ()).unwrap(), 3);
    }

    #[test]
    fn request_failure_allows_retry_and_worker_failure_closes_the_channel() {
        let worker = ProverWorker::spawn(
            "failure-test",
            || false,
            |initialized, (), ()| {
                if !*initialized {
                    *initialized = true;
                    return Err(ProofError::ProofGenerationFailed("init failed".into()));
                }
                Ok(7)
            },
        );
        assert!(worker.prove((), ()).is_err());
        assert_eq!(worker.prove((), ()).unwrap(), 7);
        let failed = ProverWorker::<(), (), ()>::spawn(
            "panic-test",
            || (),
            |_, _, _| panic!("worker failure"),
        );
        assert!(failed.prove((), ()).is_err());
        assert!(failed.prove((), ()).is_err());
    }
}
