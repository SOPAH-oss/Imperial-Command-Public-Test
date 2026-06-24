# Minecraft Utility Control Center Expansion

This package adds the full Server Project administrative platform, excluding the Expedition system as requested.

## New web page

Open after logging into the main GUI:

```text
/index.html → Control Center tab
```

The page provides the Dashboard dashboard, records editor, and global search.

## New API

```text
GET  /api/public/collections
GET  /api/public/dashboard
POST /api/public/search
GET  /api/public/:collection
POST /api/public/:collection
PUT  /api/public/:collection/:id
DELETE /api/public/:collection/:id
```

## Permissions

Owners can read and edit everything. Non-owner users need one of these permissions:

- `public` or `archive` to read Server records.
- `public_admin` to add, edit, and delete records.
- `collection:write`, for example `chronicles:write`, for collection-specific edit permission.

## Included systems

- Treasury departments, budgets, financial reports
- Chronicles, library, decrees, historical events, timeline, backups
- Intelligence bureau: players, factions, reports, sightings, watchlists, contacts
- Cartography: regions, roads, portals, ice highways, routes, regional ownership
- Hall of Records: admins, former admins, archadmins, founders, benefactors, lineage, mentors
- Relics, artifacts, banners, codices, wonders
- Governance: proposals, votes, decisions
- Citizens, census, ranks, promotions
- Postal service, notifications, news network
- Discoveries, explorer rankings, research, scholar contributions
- ID cards, certificates, archive scans, pearl network monitor, assets, emergency log

## Storage

Each module stores data in a separate JSON file named:

```text
public_<collection>.json
```

Example:

```text
public_chronicles.json
public_players.json
public_wonders.json
```

This is designed as a broad buildable foundation: the records are functional, searchable, permission-protected, and editable. More specialized buttons/workflows can be layered onto these modules later without changing the storage model.
