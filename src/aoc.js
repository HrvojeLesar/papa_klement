const fetch = require('node-fetch');
const fs = require('fs');

const CONFIG = JSON.parse(fs.readFileSync('../config.json', 'utf-8'));
const AOC_URL = CONFIG.aoc_private_leaderboard_url;
const AOC_COOKIE = CONFIG.aoc_cookie;

var AOC_GET_HAS_ERROR = false;
var AOC_RESULTS_CACHE = {};

async function get_aoc() {
    return await fetch(AOC_URL, {
        method: 'GET',
        headers: {
            cookie: `session=${AOC_COOKIE}`
        }
    })
        .then(res => {
            return res.json();
        })
        .catch(err => {
            throw err;
        })
}

function startup() {
    const _ping_aoc_interval = setInterval(() => {
        aoc();
    }, 900000);
}

function aoc() {
    console.log("Pinging AOC");
    get_aoc().then((body) => {
        AOC_GET_HAS_ERROR = false;
        AOC_RESULTS_CACHE = body;
    }).catch((err) => {
        AOC_GET_HAS_ERROR = true;
        console.log(err);
    });
}

function checkMessageArgs(message) {
    let args = message.content.split(' ');
    let day_num = Number(args[1]);
    if (isNaN(day_num)) {
        return -1;
    }

    return day_num;
}

function get_results(day) {
    const results = [];
    for (let member_id in AOC_RESULTS_CACHE.members) {
        const member = AOC_RESULTS_CACHE.members[member_id];
        let result = {
            name: member.name,
            timeDifference: 0
        }

        const member_completion = member.completion_day_level[day];
        if (member_completion != undefined) {
            if (member_completion["1"] && member_completion["2"]) {
                result.timeDifference = Number(member_completion["2"].get_star_ts) - Number(member_completion["1"].get_star_ts);
            }
        }
        if (result.timeDifference != 0) {
            results.push(result);
        }
    }

    results.sort((a, b) => {
        return a.timeDifference - b.timeDifference;
    })
    return results;
}

function buildResponse(results) {
    if (AOC_GET_HAS_ERROR) {
        return "Error getting results from AOC, check logs";
    }
    if (results.length === 0) {
        return "No results to show";
    }
    let message = `\`\`\`\n`;
    for (let result of results) {
        if (result.timeDifference != 0) {
            new Date(result.timeDifference * 1000).toISOString().substr(11, 8);
            message += `${result.name}: ${new Date(result.timeDifference * 1000).toISOString().substr(11, 8)}\n`;
        }
    }
    message += `\`\`\``;
    return message;
}

function speedrun(message) {
    let day_num = checkMessageArgs(message);
    if (day_num <= -1 || day_num > 25) {
        // set todays day or 25 if date is past 25.12.
        const today = new Date();
        const day = today.getDate();
        if (day > 25) {
            day_num = 25;
        } else {
            day_num = day;
        }
    }
    let response = buildResponse(get_results(day_num));
    message.channel.send(response);
}

exports.startup = startup;
exports.speedrun = speedrun;

const langs = [
    { lang: "Go", weight: 100, freeReroll: false },
    { lang: "C#/Java", weight: 100, freeReroll: false },
    { lang: "C++", weight: 100, freeReroll: false },
    { lang: "PHP", weight: 100, freeReroll: false },
    { lang: "Python", weight: 90, freeReroll: false },
    { lang: "JS/TS", weight: 90, freeReroll: false },
    { lang: "Scratch", weight: 2, freeReroll: false },
    { lang: "Rust", weight: 2, freeReroll: false },
    { lang: "C", weight: 2, freeReroll: false },
    { lang: "Elixir (slobodni reroll)", weight: 1, freeReroll: true },
    { lang: "Julia (slobodni reroll)", weight: 1, freeReroll: true },
    { lang: "HolyC (slobodni reroll)", weight: 1, freeReroll: true },
];

function roll() {
    const weightsPool = [];
    for (let i = 0; i < langs.length; i++) {
        weightsPool[i] = langs[i].weight + (weightsPool[i - 1] || 0);
    }

    const maxNum = weightsPool[weightsPool.length - 1]
    const randomNumber = Math.ceil(maxNum * Math.random());

    const result = langs.find((_lang, i) => {
        return weightsPool[i] >= randomNumber;
    });

    return result.lang;
}


const TIME_BETWEEN_ROLLS = 3600 * 6;

function getTimeNow() {
    return Math.floor(Date.now() / 1000);
}

let rolls = {};

function canRoll(authorId) {
    if (rolls[authorId] === undefined) {
        return true;
    }
    if (getTimeNow() - rolls[authorId].lastRolled >= TIME_BETWEEN_ROLLS) {
        return true;
    }
    return false;
}

function canReroll(authorId) {
    if (rolls[authorId] === undefined) {
        return false;
    }
    if (rolls[authorId].currentLang.freeReroll) {
        return true;
    }
    if (rolls[authorId].rerolls > 0) {
        return true;
    }
    return false;
}

function recordRoll(authorId, lang) {
    const timeNow = getTimeNow();
    if (rolls[authorId] === undefined) {
        rolls[authorId] = { currentLang: lang, rerolls: 2, rolledLangs: [lang], lastRolled: timeNow };
    } else {
        rolls[authorId] = {
            ...rolls[authorId],
            currentLang: lang,
            lastRolled: timeNow,
            rolledLangs: [
                ...rolls[authorId].rolledLangs, lang
            ]
        };
    }
}

function rollCommand(message) {
    console.log(rolls);
    const authorId = message.author.id;
    if (canRoll(authorId) === false) {
        if (rolls[authorId] !== undefined) {
            message.channel.send(rolls[authorId].currentLang);
        }
    } else {
        const lang = roll();
        if (lang === undefined) {
            message.channel.send("Dober kod pajdo");
        } else {
            recordRoll(authorId, lang);
            message.channel.send(lang);
        }
    }
}

function forceRoll(message) {
    const authorId = message.author.id;
    const lang = roll();
    recordRoll(authorId, lang);
    message.channel.send(lang);
}


function rerollCommand(message) {
    const authorId = message.author.id;
    if (canReroll(authorId) === false) {
        message.channel.send("Nemas vec rerolli!");
    } else {
        const lang = roll();
        recordRoll(authorId, lang);
        rolls[authorId].rerolls -= 1;
        message.channel.send(lang);
    }
}

function printRolls(message) {
    const authorId = message.author.id;
    if (rolls[authorId] !== undefined) {
        let response = "";
        rolls[authorId].rolledLangs.forEach((lang, i) => { 
            if (i === 0) {
                response += lang;
            } else {
                response += ", " + lang;
            }
        });
        message.channel.send(response);
    }
}

exports.roll = rollCommand;
exports.forceRoll = forceRoll;
exports.reroll = rerollCommand;
exports.rolls = printRolls;
