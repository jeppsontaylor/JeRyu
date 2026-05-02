import re
import os

with open("src/dispatch.rs", "r") as f:
    content = f.read()

print("Length of dispatch.rs:", len(content))
