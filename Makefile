.PHONY: typescript web

# TS_SRC = $(shell find web/ -type f -name '*.ts')
# JS_SRC = $(patsubst web/%.ts, web/%.js, $(TS_SRC))

web: typescript web/index.html
	browserify --entry web/app.js --outfile web/viewer.js

web/index.html: websrc/index.html
	cp $< $@

typescript:
	tsc --build tsconfig.json