var async = require('async'),
    common = require('./common'),
    config = require('./config'),
    events = require('events'),
    redis = require('redis'),
    util = require('util');

var OPs = {};
exports.OPs = OPs;

var SUBS = {};
var YAKUDON = 0;

function redis_client() {
	return redis.createClient(config.REDIS_PORT || undefined);
}
exports.redis_client = redis_client;

/* REAL-TIME UPDATES */

function Subscription(thread) {
	events.EventEmitter.call(this);
	this.thread = thread;
	if (thread == 'live')
		this.live = true;
	this.subscription_callbacks = [];
	this.promises = {};

	this.k = redis_client();
	this.k.on('error', this.on_sub_error.bind(this));
	if (this.live) {
		this.k.on('psubscribe', this.on_sub.bind(this));
		this.k.psubscribe('thread:*');
	}
	else {
		this.k.on('subscribe', this.on_sub.bind(this));
		this.k.subscribe('thread:' + thread);
	}
};

util.inherits(Subscription, events.EventEmitter);
var S = Subscription.prototype;

S.when_ready = function (cb) {
	if (this.subscription_callbacks)
		this.subscription_callbacks.push(cb);
	else
		cb(null);
};

S.promise_to = function (yaku) {
	this.promises[yaku.id] = yaku;
};

S.break_promise = function (yaku) {
	delete this.promises[yaku.id];
	for (var id in this.promises)
		if (this.promises.hasOwnProperty(id))
			return;
	/* Otherwise, this subscriptions' out of promises */
	/* Worthless subscriptions like this should just die already */
	this.seppuku();
};

S.on_sub = function () {
	var k = this.k;
	if (this.live) {
		k.removeAllListeners('psubscribe');
		k.on('pmessage', this.on_message.bind(this));
	}
	else {
		k.removeAllListeners('subscribe');
		k.on('message', this.on_message.bind(this, null));
	}
	k.removeAllListeners('error');
	k.on('error', this.sink_sub.bind(this));
	this.subscription_callbacks.forEach(function (cb) {
		cb(null);
	});
	delete this.subscription_callbacks;
};

S.on_message = function (pat, chan, msg) {
	var info = msg.match(/^(\d+),(\d+)/);
	var kind = parseInt(info[1]), num = parseInt(info[2]);
	/* Can't use this.thread since this.live might be true */
	var thread = parseInt(chan.match(/^thread:(\d+)$/)[1]);
	for (var id in this.promises)
		this.promises[id].on_update(thread, num, kind, msg);
};

S.on_sub_error = function (err) {
	console.log("Subscription error:", err.stack || err); /* TEMP? */
	this.seppuku();
	this.subscription_callbacks.forEach(function (cb) {
		cb(err);
	});
	delete this.subscription_callbacks;
};

S.sink_sub = function (err) {
	this.seppuku();
	for (var id in this.promises)
		this.promises[id].on_sink(this.thread, 'Thread unavailable.');
};

S.seppuku = function () {
	var k = this.k;
	k.removeAllListeners('error');
	k.removeAllListeners(this.live ? 'pmessage' : 'message');
	k.removeAllListeners(this.live ? 'psubscribe' : 'subscribe');
	k.quit();
	if (SUBS[this.thread] === this)
		delete SUBS[this.thread];
};

/* OP CACHE */

function on_OP_message(pat, chan, msg) {
	var op = parseInt(chan.match(/^thread:(\d+)/)[1]);
	var info = msg.split(':', 2);
	var num = parseInt(info[0]), kind = parseInt(info[1]);
	if (kind == common.INSERT_POST)
		OPs[num] = op;
}

exports.track_OPs = function (callback) {
	var k = redis_client();
	k.psubscribe('thread:*');
	k.on('psubscribe', function () {
		var r = redis_client();
		load_OPs(r, function (err) {
			r.quit();
			callback(err);
		});
	});
	k.on('pmessage', on_OP_message);
};

function load_OPs(r, callback) {
	r.keys('thread:*', function (err, keys) {
		if (err)
			return callback(err);
		async.forEach(keys, function (key, cb) {
			var m = key.match(/^thread:(\d*)(:posts$)?/);
			var op = parseInt(m[1]);
			OPs[op] = op;
			if (!m[2])
				return cb();
			r.lrange(key, 0, -1, function (err, posts) {
				if (err)
					return cb(err);
				for (var i = 0; i < posts.length; i++)
					OPs[parseInt(posts[i])] = op;
				cb();
			});
		}, callback);
	});
}

/* SOCIETY */

function Yakusoku() {
	events.EventEmitter.call(this);
	this.id = ++YAKUDON;
	/* TEMP */
	this.tag = '3:moe';
}

util.inherits(Yakusoku, events.EventEmitter);
exports.Yakusoku = Yakusoku;
var Y = Yakusoku.prototype;

Y.connect = function () {
	if (!this.r) {
		this.r = redis_client();
		this.r.on('error', console.error.bind(console));
	}
	return this.r;
};

Y.disconnect = function () {
	if (this.r) {
		this.r.quit();
		this.r.removeAllListeners('error');
	}
	this.removeAllListeners('end');
};

function forEachInObject(obj, f, callback) {
	var total = 0, complete = 0, done = false, errors = [];
	function cb(err) {
		complete++;
		if (err)
			errors.push(err);
		if (done && complete == total)
			callback(errors.length ? errors : null);
	}
	for (var k in obj) {
		if (obj.hasOwnProperty(k)) {
			total++;
			f(k, cb);
		}
	}
	done = true;
	if (complete == total)
		callback(errors.length ? errors : null);
}

Y.kiku = function (threads, on_update, on_sink, callback) {
	var self = this;
	this.on_update = on_update;
	this.on_sink = on_sink;
	forEachInObject(threads, function (thread, cb) {
		var sub = SUBS[thread];
		if (!sub) {
			sub = new Subscription(thread);
			SUBS[thread] = sub;
		}
		sub.promise_to(self);
		sub.when_ready(cb);
	}, callback);
};

Y.kikanai = function (threads) {
	for (var thread in threads) {
		var sub = SUBS[thread];
		if (sub)
			sub.break_promise(this);
	}
};

Y.reserve_post = function (op, callback) {
	this.connect().incr('postctr', function (err, num) {
		if (err)
			return callback(err);
		OPs[num] = op || num;
		callback(null, num);
	});
};

Y.insert_post = function (msg, body, ip, callback) {
	var r = this.connect();
	var tag_key = 'tag:' + this.tag;
	var self = this;
	if (!msg.num) {
		callback("No post num.");
		return;
	}
	else if (msg.op && OPs[msg.op] != msg.op) {
		delete OPs[num];
		return callback('Thread does not exist.');
	}
	var view = {time: msg.time, ip: ip, state: msg.state.join()};
	var num = msg.num, op = msg.op;
	if (msg.name)
		view.name = msg.name;
	if (msg.trip)
		view.trip = msg.trip;
	if (msg.email)
		view.email = msg.email;
	if (op)
		view.op = op;

	var key = (op ? 'post:' : 'thread:') + num;
	var bump = !op || view.email != 'sage';
	var m = r.multi();
	if (bump)
		m.incr(tag_key + ':bumpctr');
	if (msg.image) {
		if (op)
			m.hincrby('thread:' + op, 'imgctr', 1);
		else
			view.imgctr = 1;
		inline_image(view, msg.image);
		m.hset('MD5s', msg.image.MD5, msg.num);
	}
	m.hmset(key, view);
	m.set(key + ':body', body);
	if (msg.links)
		m.hmset(key + ':links', msg.links);
	if (op) {
		m.rpush('thread:' + op + ':posts', num);
	}
	else {
		op = num;
		/* Rate-limit new threads */
		if (ip && ip != '127.0.0.1') {
			m.set('ip:' + ip, op);
			m.expire('ip:' + ip, config.THREAD_THROTTLE);
		}
	}

	/* Denormalize for backlog */
	view.body = body;
	if (msg.links)
		view.links = msg.links;
	extract_image(view);
	self._log(m, op, common.INSERT_POST, [num, view]);

	m.exec(function (err, results) {
		if (err) {
			delete OPs[num];
			return callback(err);
		}
		if (!bump)
			return callback(null);
		r.zadd(tag_key + ':threads', results[0], op,
					function (err) {
			if (err)
				console.error("Bump error: " + err);
			callback(null);
		});
	});
};

Y.remove_post = function (num, callback) {
	var r = this.connect();
	var op = OPs[num];
	if (!op)
		return callback('No such post.');
	if (op == num)
		return callback('Thread deletion not supported yet.');
	r.lrem('thread:' + op + ':posts', -1, num, gone_from_thread);
	var self = this;
	function gone_from_thread(err, deleted) {
		if (err)
			return callback(err);
		if (deleted != 1)
			return callback(null, -num); /* already gone */
		var key = 'post:' + num;
		r.hset(key, 'hide', '1', function (err) {
			if (err) {
				/* Difficult to recover. Whatever. */
				console.error(err);
			}
			delete OPs[num];
			callback(null, [op, num]);

			/* In the background, try to finish the post */
			r.get(key + ':body', function (err, body) {
				if (err)
					return console.warn(err);
				var m = r.multi();
				finish_off(m, key, body);
				m.exec(function (err) {
					if (err)
						console.warn(err);
					/* Already called callback. */
				});
			});
			r.hmget(key, ['src', 'thumb'], dump_pix);
		});
	}

	function dump_pix(err, pics) {
		if (err)
			return console.warn(err);
		if (pics && pics[0] && pics[1]) {
			require('./pix').bury_image(pics[0], pics[1], check);
			function check(err) {
				if (err)
					console.warn(err);
			}
		}
	}
};

Y.remove_posts = function (nums, callback) {
	var self = this;
	async.map(nums, this.remove_post.bind(this), function (err, dels) {
		if (err)
			callback(err);
		var threads = {}, already_gone = [];
		dels.forEach(function (del) {
			if (Array.isArray(del)) {
				var op = del[0];
				if (!(op in threads))
					threads[op] = [];
				threads[op].push(del[1]);
			}
			else if (del < 0)
				already_gone.push(-del);
			else
				console.error('Unexpected del: ', del);
		});
		var m = self.connect().multi();
		for (var op in threads) {
			var nums = threads[op];
			nums.sort();
			self._log(m, op, common.DELETE_POSTS, nums);
		}
		if (already_gone.length)
			console.warn("Tried to delete missing posts: ",
					already_gone);
		m.exec(callback);
	});
};

Y.check_throttle = function (ip, callback) {
	this.connect().exists('ip:' + ip, function (err, exists) {
		if (err)
			callback(err);
		else
			callback(exists ? 'Too soon.' : null);
	});
};

Y.check_duplicate = function (MD5, callback) {
	this.connect().hexists('MD5s', MD5, function (err, exists) {
		if (err)
			callback(err);
		else
			callback(exists ? 'Duplicate image.' : false);
	});
};

Y.add_image = function (post, image, callback) {
	var r = this.connect();
	var num = post.num, op = post.op;
	if (!op)
		return callback("Can't add another image to an OP.");
	var key = 'post:' + num;
	var self = this;
	r.exists(key, function (err, exists) {
		if (err)
			return callback(err);
		if (!exists)
			return callback("Post does not exist.");
		var m = r.multi();
		self._log(m, op, common.INSERT_IMAGE, [num, image]);
		m.hmset(key, image);
		m.hincrby('thread:' + op, 'imgctr', 1);
		m.hset('MD5s', image.MD5, post.num);
		m.exec(callback);
	});
};

Y.append_post = function (post, tail, old_state, links, new_links, callback) {
	var m = this.connect().multi();
	var key = (post.op ? 'post:' : 'thread:') + post.num;
	/* Don't need to check .exists() thanks to client state */
	m.append(key + ':body', tail);
	/* XXX: fragile */
	if (old_state[0] != post.state[0] || old_state[1] != post.state[1])
		m.hset(key, 'state', post.state.join());
	if (!common.is_empty(new_links))
		m.hmset(key + ':links', new_links);
	var msg = [post.num, tail];
	if (links)
		msg.push(old_state[0], old_state[1], links);
	else if (old_state[1])
		msg.push(old_state[0], old_state[1]);
	else if (old_state[0])
		msg.push(old_state[0]);
	this._log(m, post.op, common.UPDATE_POST, msg);
	m.exec(callback);
};

function finish_off(m, key, body) {
	m.hset(key, 'body', body);
	m.del(key + ':body');
	m.hdel(key, 'state');
}

Y.finish_post = function (post, callback) {
	var m = this.connect().multi();
	var key = (post.op ? 'post:' : 'thread:') + post.num;
	/* Don't need to check .exists() thanks to client state */
	finish_off(m, key, post.body);
	this._log(m, post.op, common.FINISH_POST, [post.num]);
	m.exec(callback);
};

Y.finish_all = function (callback) {
	var r = this.connect();
	var self = this;
	r.keys('*:body', function (err, keys) {
		if (err)
			return callback(err);
		async.forEach(keys, function (body_key, cb) {
			var key = body_key.slice(0, -5);
			var m = r.multi();
			m.get(body_key);
			if (key.slice(0, 5) == 'post:')
				m.hget(key, 'op');
			m.exec(function (err, rs) {
				if (err)
					return cb(err);
				m = r.multi();
				finish_off(m, key, rs[0]);
				var n = parseInt(key.match(/:(\d+)$/)[1]);
				var op = parseInt(rs[1]) || n;
				self._log(m, op, common.FINISH_POST, [n]);
				m.exec(cb);
			});
		}, callback);
	});
};

Y._log = function (m, op, kind, msg) {
	msg.unshift(kind);
	msg = JSON.stringify(msg).slice(1, -1);
	console.log("Log:", msg);
	if (!op)
		throw new Error('No OP.');
	var key = 'thread:' + op;
	m.rpush(key + ':history', msg);
	m.hincrby(key, 'hctr', 1);
	m.publish(key, msg);
};

Y.fetch_backlogs = function (watching, callback) {
	var r = this.connect();
	var combined = [];
	forEachInObject(watching, function (thread, cb) {
		if (thread == 'live')
			return cb(null);
		var key = 'thread:' + thread + ':history';
		var sync = watching[thread];
		r.lrange(key, sync, -1, function (err, log) {
			if (err)
				return cb(err);
			combined.push.apply(combined, log);
			cb(null);
		});
	}, function (errs) {
		callback(errs, combined);
	});
};

Y.get_post_op = function (num, callback) {
	var r = this.connect();
	r.hget('post:' + num, 'op', function (err, op) {
		if (err)
			return callback(err);
		else if (op)
			return callback(null, num, op);
		r.exists('thread:' + num, function (err, exists) {
			if (err)
				callback(err);
			else if (!exists)
				callback(null, null, null);
			else
				callback(null, num, num);
		});
	});
}

Y.get_tag = function (page) {
	var r = this.connect();
	var self = this;
	var key = 'tag:' + this.tag + ':threads';
	var start = page * config.THREADS_PER_PAGE;
	var end = start + config.THREADS_PER_PAGE - 1;
	var m = r.multi();
	m.zrevrange(key, start, end);
	m.zcard(key);
	m.exec(function (err, res) {
		if (err)
			return self.emit('error', err);
		var ns = res[0];
		if (page && !ns.length)
			return self.emit('nomatch');
		self.emit('begin', res[1]);
		var reader = new Reader(self);
		reader.on('error', self.emit.bind(self, 'error'));
		reader.on('thread', self.emit.bind(self, 'thread'));
		reader.on('post', self.emit.bind(self, 'post'));
		reader.on('endthread', self.emit.bind(self, 'endthread'));
		self._get_each_thread(reader, 0, ns);
	});
};

Y._get_each_thread = function (reader, ix, nums) {
	if (!nums || ix >= nums.length) {
		this.emit('end');
		reader.removeAllListeners('endthread');
		reader.removeAllListeners('end');
		return;
	}
	var self = this;
	var next_please = function () {
		reader.removeListener('end', next_please);
		reader.removeListener('nomatch', next_please);
		self._get_each_thread(reader, ix+1, nums);
	};
	reader.on('end', next_please);
	reader.on('nomatch', next_please);
	reader.get_thread(nums[ix], false, true);
};

Y.report_error = function (info, ver, callback) {
	var r = this.connect();
	info.time = new Date().getTime();
	r.rpush('error:' + ver, JSON.stringify(info), callback);
};

/* LURKERS */

function Reader(yakusoku) {
	events.EventEmitter.call(this);
	this.y = yakusoku;
}

util.inherits(Reader, events.EventEmitter);
exports.Reader = Reader;

Reader.prototype.get_thread = function (num, redirect_ok, abbrev) {
	var r = this.y.connect();
	var key = 'thread:' + num;
	var self = this;
	r.hgetall(key, function (err, pre_post) {
		if (err)
			return self.emit('error', err);
		if (common.is_empty(pre_post)) {
			if (!redirect_ok)
				return self.emit('nomatch');
			r.hget('post:' + num, 'op',
						function (err, op) {
				if (err)
					self.emit('error', err);
				else if (!op)
					self.emit('nomatch');
				else
					self.emit('redirect', op);
			});
			return;
		}
		self.emit('begin');
		pre_post.num = num;
		with_body(r, key, pre_post, function (err, op_post) {
			if (err)
				return self.emit('error', err);
			var shonen = abbrev ? -config.ABBREVIATED_REPLIES : 0;
			var m = r.multi();
			m.lrange(key + ':posts', shonen, -1);
			if (abbrev)
				m.llen(key + ':posts');
			m.exec(function (err, r) {
				if (err)
					return self.emit('error', err);
				var omit = Math.max(r[1] + shonen, 0);
				extract_image(op_post);
				self.emit('thread', op_post, omit);
				self._get_each_reply(0, r[0]);
			});
		});
	});
};

Reader.prototype._get_each_reply = function (ix, nums) {
	if (!nums || ix >= nums.length) {
		this.emit('endthread');
		this.emit('end');
		return;
	}
	var r = this.y.connect();
	var num = nums[ix];
	var key = 'post:' + num;
	var next_please = this._get_each_reply.bind(this, ix + 1, nums);
	var self = this;
	r.hgetall(key, function (err, pre_post) {
		if (err)
			return self.emit('error', err);
		if (common.is_empty(pre_post) || pre_post.hide)
			return next_please();
		pre_post.num = num;
		with_body(r, key, pre_post, function (err, post) {
			if (err)
				return self.emit('error', err);
			extract_image(post);
			self.emit('post', post);
			next_please();
		});
	});
};

/* HELPERS */

var image_attrs;
function extract_image(post) {
	if (!image_attrs)
		image_attrs = require('./pix').image_attrs;
	if (!(image_attrs[0] in post))
		return;
	var image = {};
	image_attrs.forEach(function (key) {
		image[key] = post[key];
		delete post[key];
	});
	if (image.dims.split)
		image.dims = image.dims.split(',');
	image.size = parseInt(image.size);
	post.image = image;
}

function inline_image(post, image) {
	if (!image_attrs)
		image_attrs = require('./pix').image_attrs;
	image_attrs.forEach(function (key) {
		post[key] = image[key];
	});
}

function with_body(r, key, post, callback) {
	/* Convenience */
	post.time = parseInt(post.time);
	post.op = parseInt(post.op);

	if (post.body !== undefined)
		callback(null, post);
	else
		r.get(key + ':body', function (err, body) {
			if (err)
				return callback(err);
			if (body !== null) {
				post.body = body;
				post.editing = true;
				return callback(null, post);
			}
			// Race condition between finishing posts
			r.hget(key, 'body', function (err, body) {
				if (err)
					return callback(err);
				post.body = body;
				callback(null, post);
			});
		});
};
