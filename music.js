const ytdl = require('ytdl-core');
const ytsr = require('ytsr');
const ytpl = require('ytpl');
const validUrl = require('valid-url');
const index = require('./index.js');

let dispatcher = [];
let queue = [];

exports.dispatcher = dispatcher;
exports.queue = queue;

module.exports = {
    startup: function(client) {
        client.guilds.cache.forEach((guild) => {
            let guildId = guild.id;
            queue[guildId] = new Array();
            dispatcher[guildId] = null;
        });
    },
    
    play: async function(message, args) {
        let voiceChannel = message.member.voice.channel;
        let guildId = message.guild.id;

        if (args.length < 1) {
            console.log("COMMAND [play] | Invalid args!");
            return;
        }

        // check if url
        // check if youtube url
            // check if playlist
        // if not url search yt
        // check if search is valid
        // play found

        if (!voiceChannel) {
            return message.channel.send("Not connected to voice channel");
        }

        await handlePlayRequest(args, guildId, message.channel);

        if (message.client.voice.connections.get(guildId) === undefined) {
            voiceChannel.join().then(connection => connectionPlay(connection, guildId));
        } else {
            return;
        }
    },
    skip: function(message) {
        let guildId = message.guild.id;
        
        if (dispatcher[guildId] === null) {
            console.log("Dispatcher uninitialized!");
            message.channel.send("There is nothing to stop!");
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
        }

        dispatcher[guildId].end();
    },
    queue: function(message) {
        let guildId = message.guild.id;

        console.log(queue[guildId]);

        let queueMessage = createQueueMessage(guildId);

        if (queueMessage.length > 2000) {
            let errorMessage = `...\n**Queue too long to display!**`;
            let tmpMessage = queueMessage.slice(0, 2000 - errorMessage.length - 1);
            if (tmpMessage[tmpMessage.length - 1] === '\n') {
                tmpMessage = tmpMessage.slice(0, tmpMessage.length - 1);
            }
            tmpMessage += errorMessage;
            return message.channel.send(tmpMessage);
        }

        return message.channel.send(queueMessage);
    }
}

function connectionPlay(connection, guildId) {
    console.log("Playing: [" + queue[guildId][0].title + "] in guild with id [" + guildId + "]");
    let stream;
    if (queue[guildId][0].isYoutubeVideo) {
        // update presence
        stream = ytdl(queue[guildId][0].url, { filter: 'audioonly' });
    } else {
        // update presence 
        stream = queue[guildId][0].url;
    }
    dispatcher[guildId] = connection.play(stream, { volume: 1 });

    dispatcher[guildId].on('finish', () => {
        queue[guildId].shift();
        if (queue[guildId][0]) {
            // update presence
            connectionPlay(connection, guildId);
        } else {
            // update presence
            dispatcher[guildId].destroy();
            connection.disconnect();
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
    console.log(guildId);
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
        let length = info.items[i].duration;
        addToQueue({
            'title': title,
            'length': length,
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

    message += `Currently playing: ${queue[guildId][0].title}\n`;
    for (let i = 1; i < queue[guildId].length; i++) {
        if (queue[guildId][i].isPlaylist) {
            if (lastPlaylist != queue[guildId][i].playlistTitle) {
                lastPlaylist = queue[guildId][i].playlistTitle;
                message += `**${queue[guildId][i].playlistTitle}**\n`;
            }
            message += `> ${i}. ${queue[guildId][i].title}\n`;
        } else {
            message += `${i}. ${queue[guildId][i].title}\n`;
        }
    }

    return message;
}