const ytdl = require('ytdl-core');
const ytsr = require('ytsr');
const ytpl = require('ytpl');
const validUrl = require('valid-url');

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
        }

        await handlePlayRequest(args, guildId, message.channel);

        if (message.client.voice.connections.get(guildId) === undefined) {
            client[guildId] = message.client;
            voiceChannel.join().then((conn) => {

                connection[guildId] = conn;
                connectionPlay(guildId);
            });
        } else if (timeout[guildId].isSet) {
            timeout[guildId].isSet = false;
            connectionPlay(guildId);
        } else {
            return;
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

        console.log(lastPlayed[guildId]);
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
                console.log(queue[guildId]);
                lastPlayed[guildId] = null;
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
    console.log("Playing: [" + queue[guildId][0].title + "] in guild with id [" + guildId + "]");
    let timeStamp = queue[guildId][0].startTime;
    let stream;
    if (queue[guildId][0].isYoutubeVideo) {
        updateActivity(queue[guildId][0].title, guildId);
        stream = ytdl(queue[guildId][0].url, { filter: 'audioonly' });
    } else {
        updateActivity(queue[guildId][0].title, guildId);
        stream = queue[guildId][0].url;
    }
    dispatcher[guildId] = connection[guildId].play(stream, { volume: 1 , seek: timeStamp / 1000 });
    // dispatcher[guildId] = connection[guildId].play(stream, { volume: 1, seek: ms / 1000 });
    startEventHandlers(guildId);

    dispatcher[guildId].on('finish', () => {
        queue[guildId].shift();
        if (queue[guildId][0]) {
            updateActivity(queue[guildId][0].title, guildId);
            connectionPlay(guildId);
        } else {
            updateActivity(null, guildId);
            disconnect(guildId);
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
        let res = await ytsr(commandArgs, { limit: 1 });
        if (res.items.length < 1) {
            return channel.send("No result found!");
        }

        await handlePlayRequest(res.items[0].link, guildId, channel);
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
    let info;
    try {
        info = await ytdl.getBasicInfo(url);
    } catch (e) {
        channel.send("Video info error!");
        console.error(e);
    }
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

// ⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐
// ထ


function startEventHandlers(guildId) {
    connection[guildId].on('error', (e) => {
        console.log('C Error');
        console.log(e);
    });

    connection[guildId].on('failed', (e) => {
        console.log('C Failed');
        console.log(e);
    });

    connection[guildId].on('reconnecting', (e) => {
        console.log('C Reconnecting');
        console.log(e);
    });

    connection[guildId].on('disconnect', (e) => {
        console.log('C Disconnect');
        console.log(e);
    });

    connection[guildId].on('debug', (e) => {
        console.log('C Debug');
        console.log(e);
    });

    dispatcher[guildId].on('debug', (e) => {
        console.log('D Debug');
        console.log(e);
    });

    dispatcher[guildId].on('error', (e) => {
        console.log('D Error');
        console.log(e);
    });

    dispatcher[guildId].on('close', (e) => {
        console.log('D Close');
        console.log(e);
    });

    dispatcher[guildId].on('finish', (e) => {
        console.log('D Finish');
        console.log(e);
    });

    dispatcher[guildId].on('pipe', (e) => {
        console.log('D Pipe');
        console.log(e);
    });

    dispatcher[guildId].on('unpipe', (e) => {
        console.log('D Unpipe');
        console.log(e);
    });

        //kek pek
    streams[guildId].on('debug', (e) => {
        console.log('S Debug');
        console.log(e);
    });

    streams[guildId].on('error', (e) => {
        console.log('S Error');
        console.log(e);
    });

    streams[guildId].on('close', (e) => {
        console.log('S Close');
        console.log(e);
    });

    streams[guildId].on('finish', (e) => {
        console.log('S Finish');
        console.log(e);
    });

    streams[guildId].on('pipe', (e) => {
        console.log('S Pipe');
        console.log(e);
    });

    streams[guildId].on('unpipe', (e) => {
        console.log('S Unpipe');
        console.log(e);
    });
}