const ytdl = require('ytdl-core');
const ytsr = require('ytsr');
const ytpl = require('ytpl');
const validUrl = require('valid-url');
const fetch = require('node-fetch');

let dispatcher = [];
let queue = [];
let client = [];
let timeout = [];
let connection = [];
let lastPlayed = [];

const INFINITY = 'ထ';

exports.dispatcher = dispatcher;
exports.queue = queue;

module.exports = {
    startup: function(client) {
        client.guilds.cache.forEach((guild) => {
            let guildId = guild.id;
            queue[guildId] = new Array();
            dispatcher[guildId] = null;
            client[guildId] = null;
            timeout[guildId] = new Array();
            connection[guildId] = null;
            lastPlayed[guildId] = new Array();
        });
    },
    
    play: async function(message, args) {
        console.log("Play command");
        let voiceChannel = message.member.voice.channel;
        let guildId = message.guild.id;

        if (args.length < 1) {
            return console.log("COMMAND [play] | Invalid args!");
        }
        
        if (!voiceChannel) {
            return message.channel.send("Not connected to voice channel");
        }
        
        if (timeout[guildId].isSet) {
            clearTimeout(timeout[guildId].tout);
            // timeout[guildId].isSet = false;
        }

        // clearTimeout(timeout[guildId].tout);
        
        console.log("Handling request");
        try {
            await handlePlayRequest(args, guildId, message.channel);
            if (message.client.voice.connections.get(guildId) === undefined) {
                console.log("Prvi if");
                client[guildId] = message.client;
                console.log("channel join!");
                voiceChannel.join().then((conn) => {
                    console.log("channel joined");
    
                    connection[guildId] = conn;
                    connectionPlay(guildId);
                });
            } else if (timeout[guildId].isSet) {
                console.log(timeout[guildId].isSet);
                console.log("Drugi if");
                connectionPlay(guildId);
            } else {
                console.log("Treci if");
                return;
            }
        } catch (err) {
            console.error(err);
        }
        
    },
    skip: function(message) {
        let guildId = message.guild.id;
        
        if (dispatcher[guildId] === null) {
            console.log("Dispatcher uninitialized!");
            message.channel.send("There is nothing to skip!");
            return;
        }

        if (queue[guildId].length >= 1) {
            message.channel.send("Skipping " + queue[guildId][0].title);
        } else {
            message.channel.send("There is nothing to skip!");
        }
        dispatcher[guildId].end();
    },
    stop: function(message) {
        let guildId = message.guild.id;

        if (dispatcher[guildId] === null) {
            console.log("Dispatcher uninitialized!");
            message.channel.send("There is nothing to stop!");
            return;
        }

        if (queue[guildId].length >= 1) {
            // tu sjevaj zadnje
            lastPlayed[guildId] = [].concat(queue[guildId]);
            lastPlayed[guildId][0].startTime = queue[guildId][0].startTime + currentPlayTime(guildId) * 1000;
            queue[guildId].length = 0;
        } else if (message.client.voice.connections.get(guildId)) {
            // tu clearaj zadnje
            console.log("Clearing lastPlayed");
            lastPlayed[guildId].length = 0;
            queue[guildId].length = 0;
        } else {
            message.channel.send("There is nothing to stop!");
            return;
        }

        // stop timeout if exists
        if (timeout[guildId].isSet) {
            clearTimeout(timeout[guildId].tout);
            timeout[guildId].isSet = false;
        }
        updateActivity(null, guildId);
        dispatcher[guildId].destroy();
        connection[guildId].disconnect();
    },
    queue: function(message) {
        let guildId = message.guild.id;

        return message.channel.send(createQueueMessage(guildId));
    },

    pause: function(message) {
        let guildId = message.guild.id;

        if (dispatcher[guildId] === null) {
            console.log("Dispatcher uninitialized!");
            return;
        }

        if (dispatcher[guildId].paused) {
            console.log("Dispatcher is paused");
            return;
        }
        

        dispatcher[guildId].pause();
    },
    
    resume: function(message) {
        let guildId = message.guild.id;
        let voiceChannel = message.member.voice.channel;

        if (!voiceChannel) {
            return message.channel.send("Not connected to voice channel");
        }

        if (dispatcher[guildId] === null) {
            console.log("Dispatcher uninitialized!");
            return;
        }

        if (message.client.voice.connections.get(guildId) === undefined) {
            if (lastPlayed[guildId] != null) {
                client[guildId] = message.client;
                queue[guildId] = Object.assign({}, lastPlayed[guildId]);
                queue[guildId] = [].concat(lastPlayed[guildId]);
                lastPlayed[guildId].length = 0;
                voiceChannel.join().then((conn) => {
                    connection[guildId] = conn;
                    connectionPlay(guildId);
                });
            }
        } else if (!dispatcher[guildId].paused) {
            console.log("Dispatcher is not paused");
            return;
        } else {
            dispatcher[guildId].resume();
        }
    }
}

function connectionPlay(guildId) {
    timeout[guildId].isSet = false;
    if (queue[guildId].length == 0) {
        return;
    }
    console.log("Playing: [" + queue[guildId][0].title + "] in guild with id [" + guildId + "]");
    let timeStamp = queue[guildId][0].startTime;
    let stream;
    if (queue[guildId][0].isYoutubeVideo) {
        console.log("YT stream");
        updateActivity(queue[guildId][0].title, guildId);
        stream = ytdl(queue[guildId][0].url, { filter: 'audioonly' });
    } else {
        console.log("Unknown stream");
        updateActivity(queue[guildId][0].title, guildId);
        stream = queue[guildId][0].url;
    }
    console.log("Create dispatcher connection");
    dispatcher[guildId] = connection[guildId].play(stream, { volume: 1 , seek: timeStamp / 1000 });
    // dispatcher[guildId] = connection[guildId].play(stream, { volume: 1, seek: ms / 1000 });
    dispatcher[guildId].on('finish', () => {
        queue[guildId].shift();
        console.log("Dispatcher finish!");
        if (queue[guildId][0]) {
            console.log("Next in queue");
            updateActivity(queue[guildId][0].title, guildId);
            connectionPlay(guildId);
        } else {
            console.log("Queue is empty, init timeout");
            updateActivity(null, guildId);
            disconnect(guildId);
        }
    });
    dispatcher[guildId].on('error', (err) => {
        console.log(err.message);
        if (err.message.includes("This is a private video.")) {
            console.log(guildId);
            dispatcher[guildId].end();
        }
    });
}

async function handlePlayRequest(commandArgs, guildId, channel) {
    if (validUrl.isUri(commandArgs)) {
        // validateURL invalidates youtube.com/playlist?list=...... url (2 playlist checki zbog toga)
        if (ytdl.validateURL(commandArgs)) {
            if (!ytpl.validateID(commandArgs)) {
                console.log("Normal video");
                await handleYoutubeVideo(commandArgs, guildId, channel);
            } else {
                console.log("Playlist");
                await handlePlaylist(commandArgs, guildId, channel);
            }
        } else if (ytpl.validateID(commandArgs)) {
            console.log("Playlist2");
            await handlePlaylist(commandArgs, guildId, channel);
        } else {
            console.log("????");
            addToQueue({
                'title': commandArgs,
                'duration': 'infinity',
                'url': commandArgs,
                'isYoutubeVideo': false,
                'isPlaylist': false,
                'startTime': 0
            },
            guildId,
            channel);
        }
    } else {
        // youtube search
        let searcResults = await searchYt(createYtSearchString(commandArgs));
        console.log(searcResults);
        if (searcResults == 'No result' || searcResults == -1) {
            return channel.send("No result found!");
        }
        await handlePlayRequest(searcResults, guildId, channel);
        
        // let res = await ytsr(commandArgs, { limit: 1 });
        // if (res.items.length < 1) {
        //     return channel.send("No result found!");
        // }

        // await handlePlayRequest(res.items[0].link, guildId, channel);
    }
}

function addToQueue(queueItem, guildId, channel) {
    if (queue[guildId].length < 1) {
        queue[guildId].push(queueItem);
    } else if (queueItem.isPlaylist) {
        queue[guildId].push(queueItem);
    } else {
        channel.send("Added to queue: " + queueItem.title);
        queue[guildId].push(queueItem);
    }
}

async function handleYoutubeVideo(url, guildId, channel) {
    try {
        let info = await ytdl.getBasicInfo(url);
        addToQueue({
            'title': info.videoDetails.title,
            'duration': info.videoDetails.lengthSeconds,
            'url': url,
            'isYoutubeVideo': true,
            'isPlaylist': false,
            'startTime': parseTimestamp(url)
        },
        guildId,
        channel);
    } catch (err) {
        channel.send("Video info error!");
        throw err;
    }
}

// trenutno ne handla playlista z indexom
// samo doda celoga playlista
async function handlePlaylist(url, guildId, channel) {
    let info;
    try {
        info = await ytpl(url);
    } catch (e) {
        channel.send("Playlist info error!");
        console.error(e);
    }
    let index = getPlaylistIndex(url, info);
    if (index >= info.items.length) {
        index = 0;
    }
    let timeStamp = parseTimestamp(url);
    for (let i = index; i < info.items.length; i++) {
        addToQueue({
            'title':  info.items[i].title,
            'duration': info.items[i].duration,
            'url': info.items[i].url_simple,
            'isYoutubeVideo': true,
            'isPlaylist': true,
            'playlistTitle': info.title,
            'startTime': timeStamp,
            'playlistUrl': url
        },
        guildId,
        channel);
        startTime = 0;
    }
    channel.send("Added to queue (playlist): " + info.title);
}

function createQueueMessage(guildId) {
    let message = "";
    let lastPlaylist = null;

    if (queue[guildId].length === 0) {
        return "Queue is empty";
    }

    let totalDuration = 0;
    let playTime = currentPlayTime(guildId) + queue[guildId][0].startTime / 1000;
    let currentlyPlayingDuration;
    if (queue[guildId][0].duration == 'infinity') {
        currentlyPlayingDuration = 'infinity';
    } else {
        currentlyPlayingDuration = durationToSeconds(queue[guildId][0].duration);
    }
    message += `**Currently playing:** ${queue[guildId][0].title} **⏐⏐ ${secondsToDuration(playTime)} / `;
    if (currentlyPlayingDuration == 'infinity') {
        message += INFINITY;
        totalDuration = INFINITY;
    } else {
        message += `${secondsToDuration(currentlyPlayingDuration)}`;
        totalDuration = currentlyPlayingDuration - playTime;
    }
    message += ` ⏐⏐**\n\n`

    for (let i = 1; i < queue[guildId].length; i++) {
        if (queue[guildId][i].isPlaylist) {
            if (lastPlaylist != queue[guildId][i].playlistTitle) {
                lastPlaylist = queue[guildId][i].playlistTitle;
                message += `**${queue[guildId][i].playlistTitle}**\n`;
            }
            message += `> `;
        }
        message += `**${i}. ⏐⏐ ${secondsToDuration(totalDuration)} ⏐⏐** ${queue[guildId][i].title}\n`;
        
        if (queue[guildId][i].duration == 'infinity') {
            totalDuration = INFINITY;
        }
        if (totalDuration != INFINITY) {
            totalDuration += durationToSeconds(queue[guildId][i].duration) - queue[guildId][i].startTime / 1000;
        }
    }

    if (message.length > 2000) {
        let errorMessage = `...\n**Queue too long to display!**`;
        message = message.slice(0, 2000 - errorMessage.length - 1);
        if (message[message.length - 1] === '\n') {
            message = message.slice(0, message.length - 1);
        }
        message += errorMessage;
    }

    return message;
}

async function updateActivity(activity, guildId) {
    return client[guildId].user.setActivity(activity, { type: 'PLAYING'});
}

function disconnect(guildId) {
    // clear
    console.log("Clearing last played");
    lastPlayed[guildId].length = 0;
    timeout[guildId] = {
        'tout': setTimeout(() => {
            dispatcher[guildId].destroy();
            connection[guildId].disconnect();
            console.log("Disconnected");
            timeout[guildId].isSet = false;
        // 5 min
        }, 300000),
        'isSet': true
    }
}

function durationToSeconds(duration) {
    if (duration == INFINITY) {
        return INFINITY;
    }
    let dur = duration.split(':');
    let seconds = 0;
    for (let i = 0; i < dur.length; i++) {
        seconds += dur[i] * Math.pow(60, dur.length - i - 1);
    }
    return seconds;
}

function secondsToDuration(seconds) {
    if (seconds == INFINITY) {
        return INFINITY;
    }
    let duration = Array();
    let temp;
    if (seconds >= 3600) {
        temp = Math.floor(seconds / 3600);
        duration.push(temp);
        seconds -= 3600 * temp;
    } else {
        duration.push(0);
    }

    if (seconds >= 60) {
        temp = Math.floor(seconds / 60);
        duration.push(temp);
        seconds -= 60 * temp;
    } else {
        duration.push(0);
    }

    duration.push(seconds);

    return preetyPrintDuration(duration);
}

function preetyPrintDuration(duration) {
    let preetyPrint = Array();
    let i = 0;
    if (duration[0] == 0) {
        i++;
    }
    for (; i < 3; i++) {
        if (duration[i] < 10 && duration[i] > 0) {
            preetyPrint.push('0' + duration[i]);
        } else if (duration[i] == 0) {
            preetyPrint.push('00');
        } else {
            preetyPrint.push(duration[i].toString());
        }
    }

    return preetyPrint.join(':');
}

function currentPlayTime(guildId) {
    return Math.floor(dispatcher[guildId].streamTime / 1000);
}


// returns time in ms
function parseTimestamp(url) {
    if (url.indexOf('t=') == -1) {
        console.log('No timestamp');
        return 0;
    }

    let splitTime = url.split('t=');
    let len = splitTime.length - 1;
    let ampIndex = splitTime[len].indexOf('&');
    let time;
    if (ampIndex == -1) {
        time = splitTime[len];
    } else {
        time = splitTime[len].slice(0, ampIndex);
    }

    time = Number(time);

    if (isNaN(time)) {
        return 0;
    }

    if (time > 43200) {
        console.log("Timestamp longer than MAX youtube video length");
        return 0;
    }

    return time * 1000;
}

function getPlaylistIndex(url, info) {
    // get explicit index
    if (url.indexOf('index=') != -1) {
        let splitUrl = url.split('index=');
        let len = splitUrl.length - 1;
        let ampIndex = splitUrl[len].indexOf('&');
        let index;
        if (ampIndex == -1) {
            index = splitUrl[len];
        } else {
            index = splitUrl[len].slice(0, ampIndex);
        }

        index = Number(index);

        if (isNaN(index)) {
            return 0;
        }

        return index - 1;
    } else {
        // find index in playlist
        let splitUrl = url.split('v=');
        let len = splitUrl.length - 1;
        let ampIndex = splitUrl[len].indexOf('&');
        let id;
        if (ampIndex == -1) {
            id = splitUrl[len];
        } else {
            id = splitUrl[len].slice(0, ampIndex);
        }

        return getIndexFromInfo(info, id);
    }
}

function getIndexFromInfo(info, id) {
    let index = 0;
    for (let i = 0; i < info.items.length; i++) {
        if (info.items[i].id == id) {
            index = i;
            break;
        }
    }
    return index;
}

async function searchYt(query) {
    let return_value = -1;
    await fetch(query)
        .then(res => res.text())
        .then(body => {
            let ytInData = 'ytInitialData = ';
            let fallbackytInData = `window["ytInitialData"] =`; 
            let fallbackytInDataEnd = `window["ytInitialPlayerResponse"]`;
            let index_start = body.indexOf(ytInData);
            let index_end = -1;
            if (index_start != -1) {
                index_start += ytInData.length;
                index_end = body.indexOf('// scraper_data_end') - 3;
            } else {
                index_start = body.indexOf(fallbackytInData) + fallbackytInData.length;
                index_end = body.indexOf(fallbackytInDataEnd) - 6;
            }
            body = body.toString();
            let data = body.substring(index_start, index_end);

            let resJson = JSON.parse(data);

            let firstResult = resJson.contents.twoColumnSearchResultsRenderer.primaryContents.sectionListRenderer.contents[0].itemSectionRenderer.contents[0];
            let secondResult = resJson.contents.twoColumnSearchResultsRenderer.primaryContents.sectionListRenderer.contents[0].itemSectionRenderer.contents[1];
            if (firstResult.videoRenderer) {
                return return_value = 'https://www.youtube.com/watch?v=' + firstResult.videoRenderer.videoId;
            } else if (secondResult.videoRenderer) {
                return return_value = 'https://www.youtube.com/watch?v=' + secondResult.videoRenderer.videoId;
            } else if (firstResult.playlistRenderer) {
                return return_value = 'https://www.youtube.com/watch?v=' + firstResult.playlistRenderer.navigationEndpoint.watchEndpoint.videoId + '&list=' + firstResult.playlistRenderer.playlistId;
            } else if (secondResult.playlistRenderer) {
                return return_value = 'https://www.youtube.com/watch?v=' + secondResult.playlistRenderer.navigationEndpoint.watchEndpoint.videoId + '&list=' + secondResult.playlistRenderer.playlistId;
            } else {
                return return_value = 'No result';
            }
        })
        .catch(err => console.log(err));
    return return_value;
};

function createYtSearchString(input) {
    return 'https://www.youtube.com/results?search_query=' + input.split(' ').join('+');
}

// ⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐
// ထ