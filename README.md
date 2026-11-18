# Balans

Budżet domowy i kalkulator rebalansowania portfela inwestycyjnego.
Windows (priorytet), Linux, później Android. Język: polski / English.

## Stack
- **Tauri 2**: cała logika w Ruście (`crates/balans-core`), UI w **Svelte 5 + TypeScript**
- kwoty jako `rust_decimal` (bez floatów)
- dane lokalnie (SQLite, w przygotowaniu), kopia w jednym pliku `.balans` (JSON), np. na pendrive

## Struktura
```
crates/balans-core/   logika: waluty, kursy, model danych, rebalansowanie, eksport
src-tauri/            aplikacja Tauri (komendy wywoływane z UI)
src/                  frontend SvelteKit (SPA)
```

## Uruchomienie (dev)
Wymagania: Rust (stable), Node.js, zależności systemowe Tauri:
<https://tauri.app/start/prerequisites/>

```sh
npm install
npm run tauri dev        # aplikacja desktopowa
cargo test --workspace   # testy logiki
```

## Wydania
Tag `v*` (np. `git tag v0.1.0 && git push --tags`) buduje instalatory
Windows (NSIS/MSI) i Linux (AppImage/deb) w szkicu Release na GitHubie.

## Plan
| Etap | Zakres | Status |
|---|---|---|
| 0 | Szkielet, CI, motywy, i18n, ustawienia | ✅ |
| 1 | Rust core: model danych, waluty, SQLite, testy | 🚧 |
| 2 | Portfele i rebalansowanie (kalkulator ✅) | 🚧 |
| 3 | Moduł budżetowy | ⏳ |
| 4 | Kursy online: NBP (waluty), Stooq (GPW, ETF), CoinGecko (krypto) | ⏳ |
| 5 | Dashboard, raporty, wykresy, więcej personalizacji | ⏳ |
| 6 | Dopracowanie Linuksa, build na Androida | ⏳ |
| Później | Synchronizacja, import z banków | 💭 |
