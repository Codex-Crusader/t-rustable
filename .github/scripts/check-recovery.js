// Fails unless the self-test report shows at least one recovered document.
const fs = require("node:fs");

const report = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const step = report.steps.find((s) => s.name === "recovery");
const recovered = step?.detail?.recovered ?? 0;
console.log(`Recovered documents: ${recovered}`);
if (!report.ok || recovered < 1) process.exit(1);
