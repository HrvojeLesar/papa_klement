const Discord = require('discord.js');
const fs = require('fs');
const { exit } = require('process');
const music = require('./music.js');
const rolesManager = require('./rolesManager.js');
const unban = require('./unban.js');
const banajMatijosa = require('./banajMatijosa.js');
const aoc = require('./aoc.js')

const client = new Discord.Client();

const PREFIX = '$';

let config;

if (!fs.existsSync('../config.json')) {
    console.log('Missing config file!\nCreating new config.json!');
    fs.writeFileSync('../config.json', '{\n\t"token": "Please insert token"\n}');
    exit(0);
} else {
    try {
        config = JSON.parse(fs.readFileSync('../config.json', 'utf-8'));
    } catch (err) {
        console.log(err);
        exit(0);
    }
}

client.on('ready', () => {
    music.startup(client);
    rolesManager.startup(client);
    console.log(`Logged in as ${client.user.tag}!`);
});

client.on('message', message => {

    if (message.channel.type === 'dm' || message.author.bot) {
        return;
    }

    banajMatijosa.banaj(message, message.guild);

    if (!message.content.startsWith(PREFIX)) {
        return;
    }

    let command = message.content.split(' ');
    switch(command[0].toLowerCase()) {
        case(PREFIX + 'play'): {
            music.play(message, command.slice(1).join(' '));
            break;
        }
        case(PREFIX + 'stop'): {
            music.stop(message);
            break;
        }
        case(PREFIX + 'skip'): {
            music.skip(message);
            break;
        }
        case(PREFIX + 'queue'): {
            music.queue(message);
            break;
        }
        case(PREFIX + 'pause'): {
            music.pause(message);
            break;
        }
        case(PREFIX + 'resume'): {
            music.resume(message);
            break;
        }
        case(PREFIX + 'speedrun'): {
            aoc.speedrun(message);
            break;
        }
        case(PREFIX + 'roll'): {
            aoc.roll(message)
            break;
        }

        default: { console.log("Invalid command " + command[0]); return; }
    }
});

client.on('guildMemberUpdate', (_oldMember, newMember) => {
    rolesManager.saveRoles(newMember);
});

client.on('guildMemberAdd', (newMember) => {
    rolesManager.updateRoles(newMember);
});

client.on('guildBanAdd', (guild, user) => {
    unban.handleBan(guild, user);
});

client.login(config.token);
