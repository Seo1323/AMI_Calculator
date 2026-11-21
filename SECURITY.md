# Zgłaszanie luk bezpieczeństwa

Nie zgłaszaj luk w publicznych Issues. Użyj zakładki **Security → Report a vulnerability** w tym repozytorium (prywatne zgłoszenie dla autora).

Opisz, co zrobić, żeby błąd odtworzyć, i jaka wersja aplikacji go ma. Odpowiadam, gdy znajdę czas; to projekt prowadzony przez jedną osobę.

## Co jest w zakresie

- aplikacja desktopowa i Android (import pliku `.ami`, synchronizacja, szyfrowanie hasłem, otwieranie adresów),
- workflow wydań (`.github/workflows`).

## Czego aplikacja nie robi

Nie ma konta, serwera ani telemetrii. Dane leżą na urządzeniu. Aplikacja łączy się tylko z NBP, Yahoo Finance, CoinGecko i obligacjeskarbowe.pl po kursy oraz (na Twoje kliknięcie) z GitHubem po informację o nowej wersji.
