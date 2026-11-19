# AMI_Calculator: plan wdrożenia

Stan na 2026-11-18. Plik czyta Claude Code na początku sesji (patrz `CLAUDE.md`). Po każdym etapie zaktualizuj sekcje „Zrobione” i „Do zrobienia”.

## 1. Co robimy

Kalkulator rebalansowania portfela inwestycyjnego, dla jednej osoby (właściciel repo), na razie dla znajomych. Priorytet platform: **Windows > Linux > Android**.

- **Zakres: tylko kalkulator portfela.** Budżet domowy został wycięty (decyzja właściciela, 2026-11-18), nie wraca.
- **Portfele**: instrumenty (GPW, ETF-y, akcje, krypto), transakcje, średnia cena, zysk, alokacja docelowa, rebalansowanie (tylko dokupowanie albo pełne).
- **Kursy online**: NBP (waluty), Yahoo Finance (GPW `.WA`, giełdy zagraniczne), CoinGecko (krypto).
- **Dane lokalnie**, bez kont i chmury. Kopia w jednym pliku `.ami` (JSON), np. na pendrive.
- **Wygląd**: motyw jasny, ciemny i systemowy, kolor akcentu, gęstość, rozmiar tekstu, zaokrąglenia, języki (polski, angielski, niemiecki, hiszpański, francuski, ukraiński), tryb prywatny (ukrywa kwoty), konfigurowalny pulpit.
- Synchronizacja: **później** (model danych jest przygotowany: UUID, znaczniki czasu, miękkie usuwanie).

## 2. Stack i układ projektu

- **Tauri 2** (Rust) + **Svelte 5 / SvelteKit 3** (SPA, `adapter-static`), TypeScript.
- Kwoty zawsze jako `rust_decimal` (w UI jako stringi), nigdy float.
- SQLite przez `rusqlite` (bundled), migracje w `store.rs` (`MIGRATIONS`, obecnie wersja 2). Tabele dawnego budżetu (accounts, categories, transactions, budget_limits) zostają w migracji i w istniejących bazach jako nieużywane, żeby aktualizacja niczego nie kasowała. Nie usuwaj ich.

```
crates/ami-core/   logika bez UI: model, store, rebalance, portfolio, dashboard, providers, export
src-tauri/         aplikacja Tauri: komendy (lib.rs), pobieranie kursów (fetch.rs)
src/               frontend: routes/ (strony), lib/ (i18n, settings, komponenty, typy)
scripts/           release.mjs (wydanie), android-signing.py (podpis APK w CI)
.github/workflows/ ci.yml (testy), release.yml (instalatory)
```

## 3. Ważne miejsca (poza repozytorium)

| Co | Gdzie |
|---|---|
| Repozytorium (prywatne) | https://github.com/Seo1323/AMI_Calculator |
| Kod lokalnie | `~/Projects/AMI_Calculator` |
| Baza użytkownika (dev i release) | `~/.local/share/com.seo1323.amicalculator/ami.db` (+ `backups/`, ostatnie 10 kopii sprzed importu) |
| **Klucz do podpisu Androida** | lokalny folder właściciela poza repozytorium (`ami-release.p12` + `plik z hasłem` z hasłem i aliasem `ami`) |
| Kopia klucza w GitHub | sekrety `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` (nie da się ich odczytać, tylko użyć w buildzie) |

**Klucza nigdy nie generuj ponownie.** Nowy klucz uniemożliwia aktualizację APK na telefonach, na których jest stary. Zostaje 1 kopia w tym folderze i kopia na pendrive u właściciela.

## 4. Jak pracować

```sh
npm install
npm run tauri dev                 # aplikacja desktopowa (dev, odświeża na żywo)
cargo test --workspace            # testy logiki
cargo clippy --workspace --all-targets -- -D warnings
npm run check                     # typy frontendu
npm run tauri build -- --no-bundle   # lokalny build release, ok. 14 min (LTO)
```

**Wydanie:** `npm run release X.Y.Z` (podbija wersję, commit, tag z opisem, push). GitHub buduje 6 plików do szkicu. Sprawdź, że są **wszystkie 6** (`gh release view vX.Y.Z --json assets`), potem opublikuj jako pre-release: `gh release edit vX.Y.Z --draft=false --prerelease`. Przed wydaniem poczekaj na zielone CI na `main`.

**Test interfejsu na żywo:** debugowy plik binarny ładuje frontend z `http://localhost:1420`, więc najpierw `npm run dev`. Zrzut okna: `niri msg action screenshot-window --id <id>`, potem `wl-paste --type image/png`. **Przed wstawieniem danych demo zrób kopię `ami.db*` i potem ją przywróć.**

**Pułapki:**
- Nie używaj `pkill -f <wzorzec>`, jeśli wzorzec występuje w samym poleceniu: zabija własną powłokę. Używaj `pkill -x ami-calculator`.
- Anulowane przebiegi CI są normalne (nowszy push anuluje starszy).
- Stooq blokuje automaty (wyzwanie JS), dlatego kursy akcji idą z Yahoo (nieoficjalne API, może przestać działać, zostaje ręczne wpisywanie ceny).
- SvelteKit 3: konfiguracja w `vite.config.js` (nie `svelte.config.js`), `tsconfig.json` rozszerza `$app/tsconfig`, alias `$lib` przez `alias`.
- Na Androidzie dialogi plików zwracają `content://`, więc pliki zapisujemy przez `tauri-plugin-fs` w UI, nie przez `std::fs` w Rust.

## 5. Zrobione

| Etap | Zakres | Wydanie |
|---|---|---|
| 0 | Szkielet, CI, motywy, i18n PL/EN, ustawienia | |
| 1 | SQLite, migracje, eksport i import `.ami` (zastąp / scal), kopia przed importem | |
| 2 | Portfele, instrumenty, transakcje, średnia cena, rebalansowanie portfela | 0.1.2 |
| 4 | Kursy online: NBP, Yahoo, CoinGecko, odświeżanie automatyczne | 0.2.0 |
| 5 | Pulpit i wykresy, animowany pasek boczny, tryb prywatny, skróty Alt+1…4/H/B, układ pulpitu | 0.2.1, 0.3.0 |
| 6 (start) | Zapis plików przez `plugin-fs` (Android), commit `fa6097e` | 0.4.0 |
| 7 | Wycięcie budżetu i raportów: aplikacja to sam kalkulator portfela (Pulpit, Portfele, Rebalansowanie, Ustawienia) | 0.4.0 |

(Etap 3, budżet domowy, był w 0.2.0 i 0.3.0, a w 0.4.0 został usunięty.)

Pipeline wydań (Windows exe+msi, Linux AppImage+deb+rpm, Android arm64 APK podpisany) działa. Opublikowane pre-release: **0.1.2, 0.2.0, 0.2.1, 0.3.0**, każde z 6 plikami. Właściciel potwierdził, że APK działa na telefonie.

## 6. Do zrobienia (w tej kolejności)

1. **Ikona aplikacji**: zrobiona (waga na niebieskim tle, źródło `assets/icon.svg`, odtworzenie: `rsvg-convert -w 1024 -h 1024 assets/icon.svg -o /tmp/i.png && npm run tauri icon /tmp/i.png`, potem usuń `src-tauri/icons/ios`). Sprawdzić w kolejnym wydaniu, że APK i instalatory ją pokazują.
2. **Edycja rekordów**: zrobiona (transakcje giełdowe: ołówek w tabeli, formularz zmienia się w tryb edycji; nazwa i tolerancja portfela: ołówek przy nazwie). Backend sprawdza historię (brak nadsprzedaży) także przy edycji.
3. **Android (etap 6)**: przycisk „wstecz”, bezpieczne marginesy, wygląd na małym ekranie, test na emulatorze lub telefonie (w tym eksport/import `.ami` i CSV), odświeżanie kursów w tle.
4. **Windows**: instalator był tylko budowany w CI, nigdy uruchomiony. Poprosić o test na prawdziwym Windowsie. SmartScreen pokaże ostrzeżenie (brak płatnego certyfikatu).
5. **Linux**: przetestować AppImage i deb (do tej pory uruchamiany był tylko plik z `target/`).
6. **Z pierwotnego planu, jeszcze nie ma**: import CSV z biura maklerskiego, historia wartości portfela (wykres w czasie), rebalansowanie z podatkiem (IKE/IKZE/zwykły rachunek).
7. **Drobne**: pole daty pokazuje format amerykański (ustawienia systemu WebKit), wąskie okno wymaga przewijania tabel w bok.
8. **Później**: synchronizacja między urządzeniami; upublicznienie repozytorium (licencja, README ze zrzutami ekranu, certyfikat podpisu Windows, ewentualnie automatyczne aktualizacje).

## 7. Zasady właściciela

- Pisze po polsku, odpowiedzi po polsku.
- Maksimum personalizacji, motyw jasny i ciemny, UI dopasowany do różnych ekranów.
- Języki: polski, angielski, niemiecki, hiszpański, francuski, ukraiński (decyzja właściciela, 2026-11-19; wcześniej tylko pl i en). Każdy nowy tekst trzeba dodać we **wszystkich sześciu** plikach `src/lib/i18n/*.ts` naraz (typ `Record<keyof typeof pl, string>` tego pilnuje). Formy wobec użytkownika mają być neutralne płciowo (tryb rozkazujący, „Witaj”, w hiszpańskim „Te damos la bienvenida”, bez form typu „Bienvenido/a”, „connecté(e)”, „zalogowany/a”). Tłumaczenia de/es/fr/uk zrobił Claude i nie sprawdził ich native speaker.
- Repozytorium prywatne, dostęp tylko dla zaproszonych osób. Commity idą z adresu noreply GitHuba (konfiguracja lokalna repo), prywatnego e-maila nie wpisujemy.
- Dał szeroką swobodę w prowadzeniu planu i budowaniu lokalnie. Zawsze potwierdzaj z nim rzeczy nieodwracalne (usuwanie wydań, nowy klucz Androida, upublicznianie repo).
