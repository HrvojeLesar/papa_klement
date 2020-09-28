const ytdl = require('ytdl-core');
const ytsr = require('ytsr');
const ytpl = require('ytpl');
const validUrl = require('valid-url');

let dispatcher = [];
let queue = [];
let client = [];
let timeout = [];
let connection = [];

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

        // console.log(queue[guildId]);
        return message.channel.send(createQueueMessage(guildId));
    }
}

function connectionPlay(guildId) {
    console.log("Playing: [" + queue[guildId][0].title + "] in guild with id [" + guildId + "]");
    let stream;
    if (queue[guildId][0].isYoutubeVideo) {
        updateActivity(queue[guildId][0].title, guildId);
        stream = ytdl(queue[guildId][0].url, { filter: 'audioonly' });
    } else {
        updateActivity(queue[guildId][0].title, guildId);
        stream = queue[guildId][0].url;
    }
    dispatcher[guildId] = connection[guildId].play(stream, { volume: 1 });

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

async function handleYoutubeVideo(video, guildId, channel) {
    let info = await ytdl.getBasicInfo(video);
    let title = info.videoDetails.title;
    let duration = info.videoDetails.lengthSeconds;
    addToQueue({
        'title': title,
        'duration': duration,
        'url': video,
        'isYoutubeVideo': true,
        'isPlaylist': false,
    },
    guildId,
    channel);
}

// trenutno ne handla playlista z indexom
// samo doda celoga playlista
async function handlePlaylist(commandArgs, guildId, channel) {
    let info = await ytpl(commandArgs);
    let playlistTitle = info.title;
    for (let i = 0; i < info.items.length; i++) {
        let title = info.items[i].title;
        let url = info.items[i].url_simple;
        let duration = info.items[i].duration;
        addToQueue({
            'title': title,
            'duration': duration,
            'url': url,
            'isYoutubeVideo': true,
            'isPlaylist': true,
            'playlistTitle': playlistTitle
        },
        guildId,
        channel);
    }
    channel.send("Added to queue (playlist): " + playlistTitle);
}

function createQueueMessage(guildId) {
    let message = "";
    let lastPlaylist = null;

    if (queue[guildId].length === 0) {
        return "Queue is empty";
    }

    let totalDuration = 0;
    let playTime = currentPlayTime(guildId);
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
            totalDuration += durationToSeconds(queue[guildId][i].duration);
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

    dispatcher[guildId].on('drain', (e) => {
        console.log('D Drain');
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
}