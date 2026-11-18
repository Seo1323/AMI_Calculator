## Co nowego w 0.4
- **Sam kalkulator portfela**: budżet domowy został usunięty z aplikacji. Zostały Pulpit, Portfele, Rebalansowanie i Ustawienia.
- **Twoje dane są bezpieczne**: aktualizacja niczego nie kasuje, a stare pliki `.ami` nadal się importują (dawne dane budżetu są pomijane).
- **Pulpit**: wartość i zysk wszystkich portfeli oraz karty portfeli z alokacją obecną i docelową.
- **Android**: poprawiony zapis i odczyt plików `.ami` oraz CSV (kopia zapasowa, eksport, import).
- **Skróty klawiszowe**: Alt+1…4 (sekcje), Alt+H (ukryj kwoty), Alt+B (menu).

## Co nowego w 0.3
- **Nowy pasek boczny**: płynne zwijanie, przesuwające się podświetlenie aktywnej sekcji, przejścia między stronami (wyłączysz je opcją „Ogranicz animacje”).
- **Tryb prywatny**: ukrywa wszystkie kwoty jednym kliknięciem (Alt+H).
- **Pulpit do ułożenia**: włączanie, wyłączanie i kolejność sekcji w Ustawieniach.

## Co nowego w 0.2
- **Portfele**: instrumenty (GPW, ETF-y, akcje, krypto), transakcje kupna i sprzedaży, średnia cena zakupu, zysk/strata, alokacja docelowa i rebalansowanie jednym kliknięciem.
- **Kursy online**: NBP (waluty), Yahoo Finance (GPW i giełdy zagraniczne), CoinGecko (krypto), odświeżanie automatyczne.
- **Kopia zapasowa**: wszystkie dane w jednym pliku `.ami` (np. na pendrive).

## Instalacja

**Windows**: pobierz `AMI_Calculator_*_x64-setup.exe` i uruchom.
Jeśli pojawi się „System Windows ochronił ten komputer”, kliknij **Więcej informacji → Uruchom mimo to**
(aplikacja nie ma płatnego certyfikatu podpisu).

**Linux**:
- dowolna dystrybucja: `AMI_Calculator_*_amd64.AppImage`, potem `chmod +x AMI_Calculator_*.AppImage` i uruchom,
- Ubuntu / Debian / Mint: `sudo apt install ./AMI_Calculator_*_amd64.deb`,
- Fedora / openSUSE: `sudo dnf install ./AMI_Calculator-*.x86_64.rpm`.

**Android** (arm64): pobierz `AMI_Calculator_*_arm64.apk` na telefon i otwórz.
Telefon poprosi o zgodę na instalację z nieznanych źródeł. Wersja na Androida jest wczesna.
