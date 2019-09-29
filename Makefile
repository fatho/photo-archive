.PHONY: all typescript web cli

all: cli

cli: web
	cargo build --release

web: typescript web/index.html web/favicon.ico
	browserify --entry web/app.js --outfile web/viewer.js

web/favicon.ico: web/favicon.png
	convert $< $@

web/favicon.png: websrc/favicon.svg
	inkscape -z --export-png=$@ $<

web/index.html: websrc/index.html
	cp $< $@

typescript:
	tsc --build tsconfig.json