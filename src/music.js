const ytdl = require('ytdl-core');
const ytsr = require('ytsr');
const ytpl = require('ytpl');
const validUrl = require('valid-url');
const fetch = require('node-fetch');
const fs = require('fs');

let dispatcher = [];
let queue = [];
let client = [];
let timeout = [];
let connection = [];
let lastPlayed = [];

let lastGuildId;
let lastTextChannel;
let lastVoiceChannel;

const INFINITY = 'ထ';

const API_KEY = JSON.parse(fs.readFileSync('../config.json', 'utf-8')).yt_api_key;

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

        lastGuildId = guildId;
        lastTextChannel = message.channel;
        lastVoiceChannel = voiceChannel;

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
                client[guildId] = message.client;
                voiceChannel.join().then((conn) => {
                    console.log("channel joined");
    
                    connection[guildId] = conn;
                    connectionPlay(guildId);
                });
            } else if (timeout[guildId].isSet) {
                console.log(timeout[guildId].isSet);
                connectionPlay(guildId);
            } else {
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
    console.log(queue[guildId][0]);
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
                channel.send("Trenutno to ne dela");
                return;
                console.log("Playlist");
                await handlePlaylist(commandArgs, guildId, channel);
            }
        } else if (ytpl.validateID(commandArgs)) {
            channel.send("Trenutno to ne dela");
            return;
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
        await searchYt(commandArgs);



        // let searcResults = await searchYt(createYtSearchString(commandArgs));
        // console.log(searcResults);
        // if (searcResults == 'No result' || searcResults == -1) {
        //     return channel.send("No result found!");
        // }
        // await handlePlayRequest(searcResults, guildId, channel);
        
        // let res = await ytsr(commandArgs, { limit: 1 });
        // if (res.items.length < 1) {
        //     return channel.send("No result found!");
        // }

        // await handlePlayRequest(res.items[0].link, guildId, channel);
    }
}

function addToQueue(queueItem, guildId, channel) {
    console.log("Added to queue:");
    console.log(queueItem);
    if (queue[guildId].length < 1) {
        queue[guildId].push(queueItem);
    } else if (queueItem.isPlaylist) {
        queue[guildId].push(queueItem);
    } else {
        // channel.send("Added to queue: " + queueItem.title);
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
        // currentlyPlayingDuration = durationToSeconds(queue[guildId][0].duration);
        currentlyPlayingDuration = queue[guildId][0].duration;
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
            // totalDuration += durationToSeconds(queue[guildId][i].duration) - queue[guildId][i].startTime / 1000;
            totalDuration += queue[guildId][i].duration - queue[guildId][i].startTime / 1000;
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

// async function searchYt(query) {
//     let return_value = -1;
//     await fetch(query)
//         .then(res => res.text())
//         .then(body => {
//             let ytInData = 'ytInitialData = ';
//             let fallbackytInData = `window["ytInitialData"] =`; 
//             let fallbackytInDataEnd = `window["ytInitialPlayerResponse"]`;
//             let index_start = body.indexOf(ytInData);
//             let index_end = -1;
//             if (index_start != -1) {
//                 index_start += ytInData.length;
//                 index_end = body.indexOf('// scraper_data_end') - 3;
//             } else {
//                 index_start = body.indexOf(fallbackytInData) + fallbackytInData.length;
//                 index_end = body.indexOf(fallbackytInDataEnd) - 6;
//             }
//             body = body.toString();
//             let data = body.substring(index_start, index_end);

//             let resJson = JSON.parse(data);

//             let searchResults = resJson.contents.twoColumnSearchResultsRenderer.primaryContents.sectionListRenderer.contents[0].itemSectionRenderer.contents;

//             let firstVideo = findFirstVideo(searchResults);
//             // if (firstVideo != -1 && firstVideo.isPlaylist == false) {
//             if (firstVideo.isPlaylist == false) {
//                 return return_value = 'https://www.youtube.com/watch?v=' + searchResults[firstVideo.index].videoRenderer.videoId;
//             // } else if (firstVideo != -1 && firstVideo.isPlaylist == true) {
//             } else if (firstVideo.isPlaylist == true) {
//                 return return_value = 'https://www.youtube.com/watch?v=' + searchResults[firstVideo.index].playlistRenderer.navigationEndpoint.watchEndpoint.videoId + '&list=' + searchResults[firstVideo.index].playlistRenderer.playlistId;
//             } else {
//                 return return_value = 'No result';
//             }
//         })
//         .catch(err => console.log(err));
//     return return_value;
// };

// function createYtSearchString(input) {
//     return 'https://www.youtube.com/results?search_query=' + input.split(' ').join('+');
// }

function findFirstVideo(results) {
    for(let i = 0; i < results.length; i++) {
        if (results[i].videoRenderer) {
            return {
                'index': i,
                'isPlaylist': false
            };
        } else if (results[i].playlistRenderer) {
            return {
                'index': i,
                'isPlaylist': true
            };
        }
    }
    return -1;
}


let playlistItems = [];

async function searchYt(query) {
    let encoded_query = encodeURIComponent(query);
    let url = "https://youtube.googleapis.com/youtube/v3/search?part=snippet&maxResults=10&q=" + encoded_query + "&key=" + API_KEY;
    let result;
    await fetch(url)
        .then(res => res.text())
        .then(body => {
            result = JSON.parse(body);
        })
        .catch(err => console.log(err));
    if (result.items[0]) {
        if (result.items[0].id.videoId) {
            addToQueue(await getVideoInfo(result.items[0].id.videoId), lastGuildId, lastTextChannel);
        }
        if (result.items[0].id.playlistId) {
            await getPlaylistItems(result.items[0].id.playlistId, result.items[0].snippet.title);
            for (let i = 0; i < playlistItems.length; i++) {
                addToQueue(playlistItems[i], lastGuildId, lastTextChannel);
            }
            lastTextChannel.send("Added to queue (playlist): " + playlistItems[0].playlistTitle);
            playlistItems = [];
        }
    }
};


async function getVideoInfo(id, is_playlist = false, playlist_title = NaN) {
    let query = "https://youtube.googleapis.com/youtube/v3/videos?part=snippet%2CcontentDetails&id=" + id + "&key=" + API_KEY;
    let result;
    await fetch(query)
        .then(res => res.text())
        .then(body => {
            result = JSON.parse(body);
        })
        .catch(err => console.log(err));

    let video = {
        'title': result.items[0].snippet.title,
        'duration': parseDuration(result.items[0].contentDetails.duration),
        'url': "https://www.youtube.com/watch?v=" + id,
        'isYoutubeVideo': true,
        'isPlaylist': is_playlist,
        'playlistTitle': playlist_title,
        'startTime': 0
    };

    return video;
}


async function getPlaylistItems(id, title, nextPageToken = undefined) {
    let query; 
    if (nextPageToken == undefined) {
        query = "https://youtube.googleapis.com/youtube/v3/playlistItems?part=snippet%2CcontentDetails&maxResults=50&playlistId=" + id + "&key=" + API_KEY;
    } else {
        query = "https://youtube.googleapis.com/youtube/v3/playlistItems?part=snippet%2CcontentDetails&maxResults=50&pageToken=" + nextPageToken + "playlistId=" + id + "&key=" + API_KEY;
    }
    let result;
    await fetch(query)
        .then(res => res.text())
        .then(body => {
            result = JSON.parse(body);
        })
        .catch(err => console.log(err));
    for (let i = 0; i < result.items.length; i++) {
        console.log(result.items[i].snippet.title);
        playlistItems.push(await getVideoInfo(result.items[i].snippet.resourceId.videoId, true, title));
    }

    if (result.nextPageToken) {
        await getPlaylistItems(id, result.nextPageToken);
    }
}

function parseDuration(duration) {
    let digits = "";
    let parsed_duration = 0;
    for (let i = 0; i < duration.length; i++) {
        if (Number(duration[i])) {
            digits += duration[i];
        }

        switch (duration[i]) {
            case 'H':
                parsed_duration += Number(digits) * 3600;
                digits = "";
                break;
            case 'M':
                parsed_duration += Number(digits) * 60;
                digits = "";
                break;
            case 'S':
                parsed_duration += Number(digits);
                digits = "";
                break;
        }
    }
    return parsed_duration;
}

// ⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐⏐
// ထ