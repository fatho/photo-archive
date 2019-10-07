.PHONY: all web cli

TYPESRCIPT_SRC = $(shell find websrc/ -type f -name '*.ts')

WEB_DIST = web/viewer.js web/index.html web/favicon.ico web/favicon.png

all: cli

cli: ${WEB_DIST}
	cargo build --release

web: ${WEB_DIST}

web/viewer.js: web/app.js
	browserify --entry $< --outfile $@

web/app.js: ${TYPESRCIPT_SRC}
	tsc --build tsconfig.json

web/favicon.ico: web/favicon.png
	convert $< $@

web/favicon.png: websrc/favicon.svg
	inkscape -z --export-png=$@ $<

web/index.html: websrc/index.html
	cp $< $@
