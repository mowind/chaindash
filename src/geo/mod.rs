pub(crate) mod ipinfo;
pub(crate) mod peers;
pub(crate) mod snapshot;
pub(crate) mod store;

pub(crate) use self::{
    ipinfo::{
        IpInfoClient,
        IpInfoEntry,
    },
    snapshot::{
        CountryCount,
        GeoViewSnapshot,
    },
    store::{
        LocationEntry,
        NullPeerGeoStore,
        PeerGeoStore,
        SqlitePeerGeoStore,
    },
};

#[cfg(test)]
pub(crate) mod testutil {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{
                AtomicUsize,
                Ordering,
            },
            Arc,
            Mutex,
        },
    };

    use crossbeam_channel::{
        bounded,
        Receiver,
    };

    use super::{
        GeoViewSnapshot,
        LocationEntry,
        PeerGeoStore,
    };
    use crate::error::Result;

    /// In-memory fake used at the `PeerGeoStore` boundary in UI tests.
    #[derive(Debug)]
    pub(crate) struct FakePeerGeoStore {
        snapshot: Mutex<GeoViewSnapshot>,
        updates: Receiver<()>,
    }

    impl FakePeerGeoStore {
        pub(crate) fn new(snapshot: GeoViewSnapshot) -> Self {
            let (_tx, updates) = bounded(1);
            FakePeerGeoStore {
                snapshot: Mutex::new(snapshot),
                updates,
            }
        }
    }

    impl PeerGeoStore for FakePeerGeoStore {
        fn replace_peer_snapshot(
            &self,
            ips: Vec<String>,
        ) -> Result<Vec<String>> {
            Ok(ips)
        }

        fn update_location_cache(
            &self,
            _entries: Vec<LocationEntry>,
        ) -> Result<()> {
            Ok(())
        }

        fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot> {
            Ok(self.snapshot.lock().expect("fake store lock poisoned").clone())
        }

        fn updates(&self) -> Receiver<()> {
            self.updates.clone()
        }

        fn shutdown(&self) {}
    }

    /// Scripted store used to exercise snapshot failure and retry paths.
    #[derive(Debug)]
    pub(crate) struct ScriptedPeerGeoStore {
        reads: Mutex<VecDeque<std::result::Result<GeoViewSnapshot, String>>>,
        read_count: AtomicUsize,
        updates: Receiver<()>,
    }

    impl ScriptedPeerGeoStore {
        /// Create a store that returns the supplied read results in order.
        pub(crate) fn new(reads: Vec<std::result::Result<GeoViewSnapshot, String>>) -> Arc<Self> {
            let (_tx, updates) = bounded(1);
            Arc::new(ScriptedPeerGeoStore {
                reads: Mutex::new(reads.into_iter().collect()),
                read_count: AtomicUsize::new(0),
                updates,
            })
        }

        /// Return the number of Geo View Snapshot reads served by this store.
        pub(crate) fn read_count(&self) -> usize {
            self.read_count.load(Ordering::Relaxed)
        }
    }

    impl PeerGeoStore for ScriptedPeerGeoStore {
        fn replace_peer_snapshot(
            &self,
            ips: Vec<String>,
        ) -> Result<Vec<String>> {
            Ok(ips)
        }

        fn update_location_cache(
            &self,
            _entries: Vec<LocationEntry>,
        ) -> Result<()> {
            Ok(())
        }

        fn geo_view_snapshot(&self) -> Result<GeoViewSnapshot> {
            self.read_count.fetch_add(1, Ordering::Relaxed);
            match self.reads.lock().expect("scripted store lock poisoned").pop_front() {
                Some(Ok(snapshot)) => Ok(snapshot),
                Some(Err(error)) => Err(crate::error::ChaindashError::Other(error)),
                None => Err(crate::error::ChaindashError::Other(
                    "scripted store has no more reads".to_string(),
                )),
            }
        }

        fn updates(&self) -> Receiver<()> {
            self.updates.clone()
        }

        fn shutdown(&self) {}
    }
}
