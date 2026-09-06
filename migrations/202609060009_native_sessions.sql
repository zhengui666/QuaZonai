-- Reuse the default PostgreSQL schema of tower-sessions-sqlx-store 0.15.0.
-- Upstream: maxcountryman/tower-sessions-stores at
-- b34a2f363217c0c557ee332c8847f4e2d1b5e6b4/sqlx-store/src/postgres_store.rs
-- Copyright (c) 2024 Max Countryman. MIT; full notice in THIRD_PARTY_NOTICES.md.
-- The upstream migration opens its own pool transaction. Executing its DDL
-- through SQLx here keeps the entire deployment in the caller's transaction.
-- Session serialization, CRUD, IDs, and expiry remain entirely upstream.
create schema if not exists "tower_sessions";
create table if not exists "tower_sessions"."session"
(
    id text primary key not null,
    data bytea not null,
    expiry_date timestamptz not null
);
