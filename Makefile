.PHONY: typescript web

# TS_SRC = $(shell find web/ -type f -name '*.ts')
# JS_SRC = $(patsubst web/%.ts, web/%.js, $(TS_SRC))

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