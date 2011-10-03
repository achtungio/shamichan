var async = require('async'),
    config = require('./config'),
    child_process = require('child_process'),
    db = require('./db'),
    formidable = require('formidable'),
    fs = require('fs'),
    im = require('imagemagick'),
    path = require('path'),
    util = require('util');

function get_thumb_specs(w, h, pinky) {
	var QUALITY = config[pinky ? 'PINKY_QUALITY' : 'THUMB_QUALITY'];
	var bound = config[pinky ? 'PINKY_DIMENSIONS' : 'THUMB_DIMENSIONS'];
	var r = Math.max(w / bound[0], h / bound[1], 1);
	return {dims: [Math.round(w/r), Math.round(h/r)], quality: QUALITY};
}

exports.ImageUpload = function (clients, allocate_post, status) {
	this.clients = clients;
	this.allocate_post = allocate_post;
	this.status = status;
};

var IU = exports.ImageUpload.prototype;

var validFields = ['client_id', 'alloc'];

IU.handle_request = function (req, resp) {
	this.resp = resp;
	var form = new formidable.IncomingForm();
	form.maxFieldsSize = 2048;
	form.onPart = function (part) {
		if (part.filename && part.name == 'image')
			form.handlePart(part);
		else if (!part.filename && validFields.indexOf(part.name) >= 0)
			form.handlePart(part);
		else
			this._error('Superfluous field.');
	};
	form.parse(req, this.parse_form.bind(this));
};

IU.parse_form = function (err, fields, files) {
	if (err) {
		console.error("Upload error: " + err);
		return this.failure('Invalid upload.');
	}
	var image = files.image;
	if (!image)
		return this.failure('No image.');
	this.image = image;
	var client = this.clients[fields.client_id];
	if (!client)
		return this.failure('Invalid client id.');
	this.client = client;

	if (client.uploading) {
		this.failure('Already uploading.');
		/* previous line negated client.uploading, so restore it */
		client.uploading = true;
		return;
	}
	client.uploading = true;
	if (client.post && client.post.image)
		return this.failure('Image already exists.');
	image.ext = path.extname(image.filename).toLowerCase();
	if (image.ext == '.jpeg')
		image.ext = '.jpg';
	if (['.png', '.jpg', '.gif'].indexOf(image.ext) < 0)
		return this.failure('Invalid image format.');
	if (fields.alloc) {
		try {
			this.alloc = JSON.parse(fields.alloc);
		}
		catch (e) {
			return this.failure('Bad alloc.');
		}
	}
	else if (!client.post)
		return this.failure('Missing alloc.');
	image.imgnm = image.filename.substr(0, 256);
	client.db.check_throttle(client.ip, this.process.bind(this));
}

IU.process = function (err) {
	if (err)
		return this.failure(err);
	this.status('Verifying...');
	var image = this.image;
	var tagged_path = image.ext.replace('.', '') + ':' + image.path;
	var self = this;
	async.parallel({
		MD5: MD5_file.bind(null, image.path),
		stat: fs.stat.bind(fs, image.path),
		dims: im.identify.bind(im, tagged_path)
	}, function (err, rs) {
		if (err) {
			console.error(err);
			return self.failure('Bad image.');
		}
		image.MD5 = rs.MD5;
		image.size = rs.stat.size;
		image.dims = [rs.dims.width, rs.dims.height];
		self.client.db.check_duplicate(image.MD5,
	function (err) {
		if (err)
			return self.failure(err);
		var w = rs.dims.width, h = rs.dims.height;
		if (!w || !h)
			return self.failure('Invalid image dimensions.');
		if (w > config.IMAGE_WIDTH_MAX)
			return self.failure('Image is too wide.');
		if (h > config.IMAGE_HEIGHT_MAX)
			return self.failure('Image is too tall.');
		image.thumb_path = image.path + '_thumb';
		self.status('Thumbnailing...');
		var pinky = (self.client.post && self.client.post.op) ||
				(self.alloc && self.alloc.op);
		var specs = get_thumb_specs(w, h, pinky);
		image.dims = [w, h].concat(specs.dims);
		self.resize_image(tagged_path, image.ext, image.thumb_path,
				specs.dims, specs.quality,
	function () {
		self.status('Publishing...');
		var time = new Date().getTime();
		image.src = time + image.ext;
		image.thumb = time + '.jpg';
		var dest = path.join(config.MEDIA_DIR, 'src', image.src);
		var nail = path.join(config.MEDIA_DIR, 'thumb', image.thumb);
		async.parallel([mv_file.bind(null, image.path, dest),
				mv_file.bind(null, image.thumb_path, nail)],
				function (err, rs) {
			if (err) {
				console.error(err);
				return self.failure("Distro failure.");
			}
			image.path = dest;
			image.thumb_path = nail;
			self.publish();
		});

	});
	});
	});
}

IU.read_image_filesize = function (callback) {
	var self = this;
	fs.stat(this.image.path, function (err, stat) {
		if (err) {
			console.error(err);
			callback('Internal filesize error.');
		}
		else if (stat.size > config.IMAGE_FILESIZE_MAX)
			callback('File is too large.');
		else
			callback(null, stat.size);
	});
};

function MD5_file(path, callback) {
	child_process.exec('md5sum -b ' + path, function (err, stdout, stderr) {
		if (!err) {
			var m = stdout.match(/^([\da-f]{32})/);
			if (m)
				return callback(null, m[1]);
		}
		console.log(stdout);
		console.error(stderr);
		return callback('Hashing error.');
	});
};

function mv_file(src, dest, callback) {
	mv = child_process.spawn('/bin/mv', ['-n', src, dest]);
	mv.on('error', callback);
	mv.stderr.on('data', function (buf) {
		process.stderr.write(buf);
	});
	mv.on('exit', function (code) {
		callback(code ? 'mv error' : null);
	});
};

exports.bury_image = function (src, thumb, callback) {
	/* Just in case */
	var m = /^\d+\.\w+$/;
	if (!src.match(m) || !thumb.match(m))
		return callback('Invalid images.');
	async.parallel([mv.bind(null, 'src', src),
			mv.bind(null, 'thumb', thumb)], callback);
	function mv(p, nm, cb) {
		mv_file(path.join(config.MEDIA_DIR, p, nm),
			path.join(config.DEAD_DIR, p, nm), cb);
	}
};

IU.resize_image = function (src, ext, dest, dims, quality, callback) {
	var self = this;
	var args = [];
	if (ext == '.jpg')
		args.push('-define', 'jpeg:size=' + (dims[0] * 2) + 'x' +
				(dims[1] * 2));
	dims = dims[0] + 'x' + dims[1];
	args.push(src + '[0]', '-gamma', '0.454545', '-filter', 'box',
			'-resize', dims + '!', '-gamma', '2.2', '-strip',
			'-background', 'white', '-mosaic', '+matte',
			'-quality', ''+quality, 'jpg:' + dest);
	im.convert(args, function (err, stdout, stderr) {
		if (err) {
			console.error(stderr);
			return self.failure('Conversion error.');
		}
		if (config.DEBUG)
			setTimeout(callback, 1000);
		else
			callback();
	});
};

IU.failure = function (err_desc) {
	this.iframe_call('postForm.upload_error', err_desc);
	var image = this.image;
	if (image) {
		if (image.path)
			fs.unlink(image.path);
		if (image.thumb_path)
			fs.unlink(image.thumb_path);
	}
	if (this.client)
		this.client.uploading = false;
};

exports.image_attrs = ['src', 'thumb', 'dims', 'size', 'MD5', 'imgnm'];

IU.publish = function () {
	var client = this.client;
	var view = {};
	var self = this;
	exports.image_attrs.forEach(function (key) {
		view[key] = self.image[key];
	});
	if (client.post) {
		/* Text beat us here, discard alloc (if any) */
		client.db.add_image(client.post, view, function (err) {
			if (err || !client.post)
				return self.failure("Publishing failure.");
			client.post.image = view;
			client.uploading = false;
			self.iframe_call('postForm.upload_complete', view);
		});
		return;
	}
	self.allocate_post(self.alloc, view, client, function (err, alloc) {
		if (err) {
			console.error(err);
			return self.failure('Bad post.');
		}
		client.uploading = false;
		self.iframe_call('postForm.on_allocation', alloc);
	});
};

IU.iframe_call = function (func, param) {
	var resp = this.resp;
	param = param ? JSON.stringify(param) : '';
	resp.writeHead(200, {'Content-Type': 'text/html; charset=UTF-8'});
	resp.end('<!doctype html>\n<title></title>\n<script>'
		+ 'parent.' + func + '(' + param + ');</script>');
};
