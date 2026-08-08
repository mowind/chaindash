pub(crate) mod ipinfo;
pub(crate) mod peers;
pub(crate) mod snapshot;
pub(crate) mod store;

pub(crate) use self::{
    ipinfo::{
        IpInfoClient,
        IpInfoEntry,
    },
    snapshot::GeoViewSnapshot,
    store::{
        LocationEntry,
        NullPeerGeoStore,
        PeerGeoStore,
        SqlitePeerGeoStore,
    },
};

#[cfg(test)]
pub(crate) mod testutil {
    use std::sync::Mutex;

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
}
