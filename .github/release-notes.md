## Co nowego w 0.5.0
- **Historia wartości portfela**: na stronie portfela jest wykres wartości w czasie oraz linia „Wpłacone” (suma kupna z opłatami minus sprzedaż, liczona z transakcji po dzisiejszych kursach walut). Wartość zapisuje się raz dziennie przy otwarciu pulpitu lub strony portfela, gdy wycena jest kompletna.
- **Średnie oprocentowanie** na pulpicie, liście portfeli i stronie portfela: średnia ważona wartością z ręcznie wpisanych stóp (obligacje, gotówka, lokaty). ETF-y i akcje bez wpisanej stopy się nie liczą.
- **Do wiedzenia**: historia wartości zaczyna się od pierwszego uruchomienia tej wersji (nie ma kursów historycznych) i nie wchodzi do kopii `.ami`, więc po imporcie na innym urządzeniu zaczyna od zera. Tłumaczenia nowych tekstów (de/es/fr/uk) nie były sprawdzone przez osobę znającą język.

## Co nowego w 0.4.9
- **Poprawka: portfel z uszkodzoną historią transakcji** (np. sprzedaż większa niż liczba posiadanych jednostek po scaleniu kopii z dwóch urządzeń) nie wyłącza już pulpitu ani listy portfeli. Taki portfel jest oznaczony „Problem z danymi portfela”, a na jego stronie widać baner i tabelę transakcji, w której błędną transakcję można poprawić lub usunąć.

## Co nowego w 0.4.8
- **Poprawka: zamykanie aplikacji po literówce w kwocie.** Wpisanie absurdalnie dużej ilości, ceny, opłaty, wpłaty lub kursu (np. za dużo zer) mogło zamknąć aplikację, także przy każdym kolejnym uruchomieniu. Takie wartości (powyżej 1 000 000 000 000) są teraz odrzucane z komunikatem.
- **Poprawka: import `.ami`** sprawdza rekordy tak samo jak zwykły zapis, więc ręcznie zmieniony plik nie wprowadzi niepoprawnych danych.
- **Poprawka: kursy** — odświeżenie, w którym dostawca nie zwrócił stopy zwrotu z 12 miesięcy lub dywidendy, nie kasuje już wcześniej znanych wartości.
- **Poprawka: cele alokacji** zapisują się w całości albo wcale, a ten sam instrument nie może pojawić się dwa razy.
- **Poprawka: duplikaty.** Gdy zapis portfela z celami albo instrumentu z pierwszą pozycją nie powiódł się w drugim kroku, ponowne kliknięcie tworzyło kopię. Teraz pierwszy krok jest cofany.
- **Poprawka: wygląd** — czarny styl ciemnego motywu (AMOLED) ma teraz też czarny pasek boczny, a strona portfela ładuje się szybciej i nie pokazuje wyniku po przejściu do innego portfela.
- **Do wiedzenia**: portfel z absurdalną transakcją zapisaną w starszej wersji pokaże błąd zamiast widoku (pulpit też), a takiej transakcji nie da się wtedy usunąć w interfejsie.

## Co nowego w 0.4.7
- **Więcej opcji wyglądu** (Ustawienia → Wygląd): czcionka (domyślna, szeryfowa, monospace), cienie kart (brak, delikatne, wyraźne), styl ciemnego motywu (miękki albo czarny AMOLED), szerokość treści (wąska, normalna, pełna), poświata koloru akcentu w tle, paleta dla daltonistów (niebieski i pomarańczowy zamiast zieleni i czerwieni) oraz pokazywanie groszy w kwotach.
- **Do wiedzenia**: opcje zapisują się razem z resztą ustawień, więc trafiają też do kopii `.ami`. Tłumaczenia nowych tekstów (de/es/fr/uk) nie były sprawdzone przez osobę znającą język.

## Co nowego w 0.4.6
- **Płynniejsze przewijanie na Linuksie (niri)**: na natywnym Waylandzie GL się nie uruchamiał, a strona była rysowana programowo w czterokrotnie większej rozdzielczości (ok. 3 klatki na sekundę). Aplikacja startuje teraz przez XWayland (ok. 60 klatek na sekundę). Wyłączysz to zmienną `AMI_DISPLAY=wayland`.
- **Tryb wydajny** (Ustawienia): wyłącza poświatę, cienie, gradienty i animacje. W trybie automatycznym włącza się sam, gdy wykryje bardzo wolne klatki.
- **Suwaki celu balansu** przy zakładaniu i w ustawieniach portfela, nowy, bardziej stylowy wygląd, nazwa „AMI Calculator” w interfejsie.
- **Poprawka**: strona startowa i podświetlenie aktywnej pozycji menu w zainstalowanej aplikacji.
- **Do wiedzenia**: efekt przełączenia na XWayland sprawdzony tylko na niri; na innych pulpitach nic się nie zmienia.

## Co nowego w 0.4.5
- **Wykres kołowy na stronie portfela**: „Podział aktywów” pokazuje, ile wartości portfela jest w ETF-ach, akcjach, obligacjach, krypto, gotówce i lokatach, z dokładnymi udziałami i kwotami w tabeli obok. Najechanie na wycinek pokazuje szczegóły. Działa w trybie jasnym, ciemnym i prywatnym.
- **Poprawka: data wykupu** obligacji i lokat nie pokazuje już godziny („1 lis 2036, 01:00”) i nie przesuwa się o dzień w innych strefach czasowych.
- **Poprawka: obligacje bez wpisanej ceny** nie liczą się już jako 0 zł. Do czasu wpisania ceny są wyceniane po koszcie zakupu, a na stronie portfela pojawia się o tym komunikat.
- **Poprawka: nieprawidłowe transakcje**: ilość równa zero lub ujemna oraz ujemna cena albo opłata są odrzucane z komunikatem. Wcześniej sprzedaż zerowej ilości mogła zawiesić widok portfela.
- **Wersja 0.4.4 nie została opublikowana**, więc ta wersja zawiera także jej zmiany opisane niżej (listy z wyszukiwaniem, obligacje z oferty Ministerstwa Finansów, lokaty, oprocentowanie, dodawanie pozycji w jednym kroku, skala interfejsu).

## Co nowego w 0.4.4 (nie wydano osobno, zmiany są w 0.4.5)
- **Wybór z list zamiast wpisywania**: po wybraniu rodzaju (ETF, akcje, obligacje, krypto, gotówka, lokata) wybierasz konkretny instrument z listy z wyszukiwaniem (ETF-y, 32 akcje z GPW i zagranicy, kryptowaluty, waluty, serie obligacji). Zawsze możesz też wpisać własny instrument.
- **Obligacje skarbowe**: po wybraniu serii (OTS, ROR, DOR, TOS, COI, ROS, EDO, ROD) aplikacja sama podpowiada oprocentowanie pierwszego okresu, kapitalizację i cenę 100 zł z aktualnej oferty Ministerstwa Finansów (obligacjeskarbowe.pl; bez internetu użyje zapisanej oferty z listopada 2026). Wszystko możesz zmienić. Aplikacja liczy datę wykupu (1. dzień miesiąca), symbol (np. EDO1136) i ile zostało do wykupu.
- **Lokata bankowa**: nowy rodzaj z terminem, datą startu, kapitalizacją i ręcznym oprocentowaniem.
- **Oprocentowanie przy ETF-ach, akcjach i krypto**: stopa zwrotu z ostatnich 12 miesięcy i (dla ETF-ów) dywidenda, pobierane razem z kursem.
- **Dodawanie pozycji w jednym kroku**: przy dodawaniu instrumentu wybierasz portfel, ilość (liczbę obligacji albo kwotę), cenę i datę. Cena akcji, ETF-ów i krypto uzupełnia się sama z bieżącego kursu i można ją zmienić.
- **Skala interfejsu** 50–200% z gotowymi ustawieniami (Ustawienia → Wygląd), niezależna od rozmiaru tekstu.
- **Do wiedzenia**: oprocentowanie obligacji z oferty dotyczy pierwszego okresu (kolejne zależą od inflacji lub stopy NBP) i nie zmienia wartości portfela, tylko jest zapisywane i pokazywane. Cena z kursu jest ceną bieżącą, więc przy wcześniejszej dacie zakupu popraw ją ręcznie. Tłumaczenia nowych tekstów nie były jeszcze sprawdzone przez osobę znającą język.

## Co nowego w 0.4.3
- **Nowe języki**: niemiecki, hiszpański, francuski i ukraiński (Ustawienia → Język i formaty). Tłumaczenia nie były jeszcze sprawdzone przez osobę znającą język, więc daj znać o błędach.
- **Różne rozmiary ekranu i skale**: menu zwija się do ikon w średnich oknach, treść jest wyśrodkowana na dużych monitorach, a na 4K przy skali 100% tekst jest większy. Kalkulator rebalansowania na telefonie pokazuje pozycje jako karty. Zwijanie menu i chowanie kolumn uwzględnia też ustawiony rozmiar tekstu.

## Co nowego w 0.4.2
- **Rebalansowanie na telefonie**: pozycje kalkulatora są małymi kartami z podpisanymi polami (wcześniej pole „Cel %” było poza ekranem), tabela wyników chowa mniej ważne kolumny.

## Co nowego w 0.4.1
- **Własna ikona aplikacji** (waga) na wszystkich platformach.
- **Edycja**: transakcje giełdowe (ołówek w tabeli) oraz nazwa i tolerancja portfela. Zmiana, po której wyszłaby sprzedaż niekupionych akcji, jest odrzucana.
- **Różne rozmiary ekranu**: menu zwija się do ikon w średnich oknach, mniej ważne kolumny tabel chowają się na telefonie, pola formularzy zajmują całą szerokość, większe przyciski na ekranach dotykowych, poprawione marginesy na Androidzie.
- **Poprawka**: pulpit na wąskim ekranie zajmuje całą szerokość.

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
