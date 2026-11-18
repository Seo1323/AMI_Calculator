# AMI_Calculator

Budżet domowy i kalkulator rebalansowania portfela inwestycyjnego.
Windows (priorytet), Linux, później Android. Język: polski / English.

## Stack
- **Tauri 2**: cała logika w Ruście (`crates/ami-core`), UI w **Svelte 5 + TypeScript**
- kwoty jako `rust_decimal` (bez floatów)
- dane lokalnie w SQLite (`ami.db` w katalogu danych aplikacji), kopia w jednym pliku `.ami` (JSON), np. na pendrive

## Struktura
```
crates/ami-core/   logika: waluty, kursy, model danych, rebalansowanie, eksport
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
`npm run release 0.2.0` podbija wersję, tworzy tag i wypycha go. GitHub buduje instalatory
Windows (exe/msi), Linux (AppImage/deb/rpm) i Android (arm64 APK) do szkicu Release.

## Plan
| Etap | Zakres | Status |
|---|---|---|
| 0 | Szkielet, CI, motywy, i18n, ustawienia | ✅ |
| 1 | Rust core: model danych, waluty, SQLite, eksport/import, testy | ✅ |
| 2 | Portfele, instrumenty, transakcje, alokacja, rebalansowanie portfela | ✅ |
| 3 | Budżet: konta, kategorie, transakcje, przelewy, limity, podsumowanie miesiąca | ✅ |
| 4 | Kursy online: NBP (waluty), Stooq (GPW, ETF), CoinGecko (krypto) | ⏳ |
| 5 | Dashboard, raporty, wykresy, więcej personalizacji | ⏳ |
| 6 | Dopracowanie Linuksa, build na Androida | ⏳ |
| Później | Synchronizacja, import z banków | 💭 |
