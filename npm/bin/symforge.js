#!/usr/bin/env node
"use strict";

const { createLauncher } = require("./launcher.js");

createLauncher().main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (err) => {
    console.error(err.message);
    process.exit(1);
  }
);
