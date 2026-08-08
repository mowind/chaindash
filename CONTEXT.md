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
The persisted per-IP enrichment data used to associate a peer with its country and optional internal map position.
_Avoid_: peer snapshot.

**Geo View Snapshot**:
The read-only view assembled from the current peer snapshot and location cache for rendering the dotted globe.
_Avoid_: raw SQLite rows, collector state.

**Located Peer**:
A peer whose cached location contains enough valid data to be plotted on the dotted globe.
_Avoid_: precise geolocation; the UI does not expose exact coordinates.
