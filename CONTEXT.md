# ChainDash Context

ChainDash monitors configured PlatON nodes and presents their current network and system state in a terminal dashboard.

## Language

**Monitored Node**:
A PlatON node configured by the user through `--url` and queried by the dashboard.
_Avoid_: peer, connected node.

**Peer**:
A network node reported by a monitored node through `admin_peers`.
_Avoid_: monitored node.

**Peer Snapshot**:
The most recent successful, merged and IP-deduplicated set of peers across all monitored nodes.
_Avoid_: peer history, peer cache.

**Location Cache**:
The persisted per-IP enrichment data used to associate a peer with its country and optional geographic coordinates.
_Avoid_: peer snapshot, peer country distribution.

**Geo View Snapshot**:
The read-only geographic view assembled from the current peer snapshot and location cache for dashboard presentation.
_Avoid_: raw SQLite rows, collector state.

**Country Code**:
A normalized uppercase two-letter country identifier associated with a peer. Missing or invalid values are classified as Unknown Country.
_Avoid_: country name, arbitrary provider country string.

**Unknown Country**:
The classification for a peer that has no valid Country Code, regardless of why enrichment data is unavailable.
_Avoid_: unlocated peer, enrichment failure.

**Peer Country Distribution**:
The grouping of the current Peer Snapshot by Country Code. Each IP-deduplicated peer is counted once, including peers classified as Unknown Country.
_Avoid_: Peer Map, monitored node distribution, geographic location.
