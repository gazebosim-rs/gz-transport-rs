#!/usr/bin/env python3
"""Cargo applies this only to owned workspace packages, never registry/Git deps."""
import os
import sys

os.execvp(sys.argv[1], [*sys.argv[1:], '-C', 'instrument-coverage'])
