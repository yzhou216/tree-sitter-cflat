# SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
# SPDX-License-Identifier: GPL-3.0-or-later

# The tests build the grammar from ../src unless GRAMMAR_DIR names an
# existing directory holding libtree-sitter-cflat.so.

EMACS ?= emacs
CC ?= cc

ROOT := $(realpath $(dir $(lastword $(MAKEFILE_LIST)))/..)
GRAMMAR_DIR ?= $(CURDIR)/.grammar
GRAMMAR_LIB := $(GRAMMAR_DIR)/libtree-sitter-cflat.so

# Defaults to the grammar's own corpus, extracted afresh on every run; point
# it at `scripts/fuzz.rs --dump` output for a larger sample.
STRESS_DIR ?= $(CURDIR)/.stress

BATCH := $(EMACS) -Q --batch -L $(CURDIR)
RUN := CFLAT_GRAMMAR_DIR=$(GRAMMAR_DIR) $(BATCH)

.PHONY: all compile grammar test stress clean

all: compile

compile:
	$(BATCH) --eval '(setq byte-compile-error-on-warn t)' \
	         -f batch-byte-compile cflat-ts-mode.el

grammar: $(GRAMMAR_LIB)

$(GRAMMAR_LIB): $(ROOT)/src/parser.c
	@mkdir -p $(GRAMMAR_DIR)
	$(CC) -shared -fPIC -O2 -std=c11 -I $(ROOT)/src $(ROOT)/src/parser.c -o $@

test: compile $(GRAMMAR_LIB)
	$(RUN) -l test/cflat-ts-mode-tests.el -f ert-run-tests-batch-and-exit

stress: compile $(GRAMMAR_LIB)
	@if [ "$(STRESS_DIR)" = "$(CURDIR)/.stress" ]; then \
	    rm -rf "$(STRESS_DIR)"; \
	    cargo -Zscript $(ROOT)/scripts/corpus-sources.rs "$(STRESS_DIR)" >/dev/null; \
	fi
	$(RUN) -l test/indent-stress.el -f cflat-indent-stress-main "$(STRESS_DIR)"

clean:
	rm -f *.elc
	rm -rf $(CURDIR)/.grammar $(CURDIR)/.stress
