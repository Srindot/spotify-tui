# spotify-tui
#   make            build target/release/spotify_tui
#   make install    install to ~/.local (PREFIX=/usr/local for everyone; needs sudo):
#                   the binary, plus a launcher entry (rofi, app menus) and icon
#   make uninstall  remove it again
#   make test       run the tests
#
# The launcher opens it in TERMINAL (default kitty): make install TERMINAL=alacritty
# Cargo features: make FEATURES="image notify"

PREFIX   ?= $(HOME)/.local
TERMINAL ?= kitty
FEATURES ?= image
BINDIR   := $(PREFIX)/bin
APPDIR   := $(PREFIX)/share/applications
ICONDIR  := $(PREFIX)/share/icons/hicolor/scalable/apps
BIN      := spotify_tui
NAME     := spotify-tui

.PHONY: build install uninstall test clean

build:
	cargo build --release --locked -p spotify_player --features "$(FEATURES)"

install: build
	mkdir -p $(DESTDIR)$(BINDIR)
	install -m 755 target/release/$(BIN) $(DESTDIR)$(BINDIR)/$(BIN)
	mkdir -p $(DESTDIR)$(APPDIR) $(DESTDIR)$(ICONDIR)
	install -m 644 assets/$(NAME).svg $(DESTDIR)$(ICONDIR)/$(NAME).svg
	@termpath=$$(command -v $(TERMINAL) || echo $(TERMINAL)); \
	case "$(TERMINAL)" in \
		kitty) exec_line="$$termpath --class=$(NAME) --title=Spotify $(BINDIR)/$(BIN)";; \
		*)     exec_line="$$termpath -e $(BINDIR)/$(BIN)";; \
	esac; \
	printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=Spotify TUI' \
		'GenericName=Music player' 'Comment=Spotify in the terminal' \
		"Exec=$$exec_line" 'Icon=$(NAME)' 'Terminal=false' 'Categories=Audio;Music;Player;AudioVideo;' \
		'Keywords=spotify;music;player;tui;' 'StartupWMClass=$(NAME)' \
		> $(DESTDIR)$(APPDIR)/$(NAME).desktop
	-@update-desktop-database $(DESTDIR)$(APPDIR) 2>/dev/null
	@echo "installed $(DESTDIR)$(BINDIR)/$(BIN) and its launcher entry"
	@case ":$$PATH:" in *":$(BINDIR):"*) ;; \
		*) echo "note: $(BINDIR) is not on your PATH; add: export PATH=\"$(BINDIR):\$$PATH\"";; esac

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN) $(DESTDIR)$(APPDIR)/$(NAME).desktop $(DESTDIR)$(ICONDIR)/$(NAME).svg

test:
	cargo test --release --locked -p spotify_player --features "$(FEATURES)"

clean:
	cargo clean
