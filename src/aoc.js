const fetch = require('node-fetch');
const fs = require('fs');

const CONFIG = JSON.parse(fs.readFileSync('../config.json', 'utf-8'));
const AOC_URL = CONFIG.aoc_private_leaderboard_url;
const AOC_COOKIE = CONFIG.aoc_cookie;

var AOC_GET_HAS_ERROR = false;
var AOC_RESULTS_CACHE = {};

const PING_AOC_INTERVAL = setInterval(() => {
    aoc();
},  900000);

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

// nepotrebno 
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

aoc();

exports.speedrun = speedrun;